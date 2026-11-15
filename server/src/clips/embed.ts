import { rename, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { desc, eq } from 'drizzle-orm';
import type { FastifyBaseLogger } from 'fastify';
import type { Db } from '../db/client.js';
import { clips } from '../db/schema.js';
import { embedFileName, mediaFileName } from './files.js';
import { encodeEmbed, probeVideo } from './media.js';

/**
 * Discord's crawler downloads the whole og:video file, several times in parallel, and drops the
 * embed (title card included) when a download runs past a few seconds. A clip's stored video
 * comes from the owner's home connection, so the embed copy is kept small enough to arrive in
 * about a second.
 */
export const EMBED_TARGET_BYTES = 8 * 1024 * 1024;
/** A copy that still came out bigger than this (bitrate overshoot) isn't used. */
export const EMBED_MAX_BYTES = 10 * 1024 * 1024;
const MAX_VIDEO_KBPS = 2500;
/** Below this the picture is too poor to be worth it: long clips embed with their thumbnail only. */
const MIN_VIDEO_KBPS = 500;
const AUDIO_KBPS = 96;
const MAX_FPS = 30;

export interface EmbedPlan {
  width: number;
  height: number;
  videoKbps: number;
}

/** Fits `size` so its shorter side is at most `short`, keeping the aspect ratio and even sides. */
function fit(width: number, height: number, short: number): { width: number; height: number } {
  const even = (n: number) => Math.max(2, Math.round(n / 2) * 2);
  const s = Math.min(width, height);
  if (s <= short) return { width: even(width), height: even(height) };
  const k = short / s;
  return { width: even(width * k), height: even(height * k) };
}

/** The embed copy's size and bitrate for a clip, or null when the clip is too long for one. */
export function embedPlan(clip: { width: number; height: number; durationMs: number }): EmbedPlan | null {
  const seconds = Math.max(1, clip.durationMs / 1000);
  // 5% for the MP4 container and the encoder overshooting its average.
  const budgetKbps = Math.floor(((EMBED_TARGET_BYTES * 8) / seconds / 1000) * 0.95) - AUDIO_KBPS;
  const videoKbps = Math.min(MAX_VIDEO_KBPS, budgetKbps);
  if (videoKbps < MIN_VIDEO_KBPS) return null;
  // A tighter budget looks better at a smaller size.
  return { ...fit(clip.width, clip.height, videoKbps >= 1500 ? 720 : 540), videoKbps };
}

/**
 * Makes `<id>.embed.mp4` from `source`. Returns whether there is one now; failures are logged and
 * never thrown, because a missing copy only means the embed falls back to the full video.
 */
export async function createEmbed(opts: {
  clipsDir: string;
  clip: { id: string; width: number; height: number; durationMs: number };
  source: string;
  threads: number;
  log: FastifyBaseLogger;
}): Promise<boolean> {
  const { clipsDir, clip, log } = opts;
  const plan = embedPlan(clip);
  if (!plan) return false;
  const out = join(clipsDir, embedFileName(clip.id));
  const tmp = join(clipsDir, `.${embedFileName(clip.id)}.tmp`);
  try {
    const info = await probeVideo(opts.source);
    await encodeEmbed(opts.source, tmp, {
      ...plan,
      audioKbps: AUDIO_KBPS,
      fps: info.fps !== null && info.fps > MAX_FPS + 1 ? MAX_FPS : null,
      threads: opts.threads,
    });
    const size = (await stat(tmp)).size;
    if (size > EMBED_MAX_BYTES) {
      log.warn({ clipId: clip.id, size }, 'embed copy came out too big; not used');
      await rm(tmp, { force: true });
      return false;
    }
    await rename(tmp, out);
    return true;
  } catch (err) {
    log.error({ err, clipId: clip.id }, 'embed copy failed');
    await rm(tmp, { force: true });
    return false;
  }
}

/** Whether the clip has its embed copy. */
export async function hasEmbed(clipsDir: string, id: string): Promise<boolean> {
  return !!(await stat(join(clipsDir, embedFileName(id))).catch(() => null));
}

/**
 * Makes embed copies for clips uploaded before they existed, newest first, one at a time, in the
 * background. Clips too long for a copy are passed over; a failed one is tried again next start.
 */
export class EmbedBackfill {
  private stopped = false;
  private running: Promise<void> | null = null;

  constructor(private opts: { db: Db; clipsDir: string; threads: number; log: FastifyBaseLogger }) {}

  start(): void {
    this.running = this.run().catch((err) => this.opts.log.error({ err }, 'embed backfill crashed'));
  }

  /** Resolves once the pass is over (for tests). */
  async idle(): Promise<void> {
    await this.running;
  }

  async stop(): Promise<void> {
    this.stopped = true;
    await this.running;
  }

  private async run(): Promise<void> {
    const { db, clipsDir, log } = this.opts;
    const rows = await db
      .select({ id: clips.id, kind: clips.kind, mediaVersion: clips.mediaVersion, width: clips.width, height: clips.height, durationMs: clips.durationMs })
      .from(clips)
      .where(eq(clips.kind, 'clip'))
      .orderBy(desc(clips.createdAt), desc(clips.id));
    let made = 0;
    for (const row of rows) {
      if (this.stopped) return;
      if (!embedPlan(row) || (await hasEmbed(clipsDir, row.id))) continue;
      const source = join(clipsDir, mediaFileName(row));
      if (!(await stat(source).catch(() => null))) continue; // deleted meanwhile
      if (!(await createEmbed({ clipsDir, clip: row, source, threads: this.opts.threads, log }))) continue;
      const [still] = await db.select({ id: clips.id }).from(clips).where(eq(clips.id, row.id));
      if (still) made++;
      else await rm(join(clipsDir, embedFileName(row.id)), { force: true }); // deleted while encoding
    }
    if (made > 0) log.info(`embed backfill: made ${made} embed copies`);
  }
}
