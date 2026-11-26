import { rename, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { and, desc, eq, gte, lte } from 'drizzle-orm';
import type { FastifyBaseLogger } from 'fastify';
import type { Db } from '../db/client.js';
import type { DiskGuard } from '../disk.js';
import { clips } from '../db/schema.js';
import { embedFileName, mediaFileName } from './files.js';
import { encodeEmbed, probeVideo } from './media.js';

/**
 * Discord's crawler downloads the whole og:video file and drops the embed (title card included)
 * when that takes too long. Tested in Nov 2026 against a home connection with 188 Mbps upload:
 * 95 MB embedded fine (Discord waited 7 s for it), 680 MB got no embed at all. Short clips get a
 * small copy that arrives in about a second and is made while the uploader waits.
 */
export const EMBED_TARGET_BYTES = 8 * 1024 * 1024;
/**
 * Clips too long for a decent 8 MB copy get a bigger one, made in the background: their share
 * link is handed out once it's ready (`shareReady`).
 */
export const LONG_EMBED_TARGET_BYTES = 50 * 1024 * 1024;
/**
 * Bump whenever the copy's settings change: clips record the version their copy was made with,
 * and at start the backfill remakes every copy made with an older one, replacing it in place.
 * 1: video capped at 2.5 Mbps. 2: capped at 6 Mbps.
 */
export const EMBED_VERSION = 2;
/**
 * A clip without a copy (too long even for a long one, or its copy failed) puts its full video in
 * the embed only up to this size. Above it the embed is the thumbnail as a large image.
 */
export const FULL_VIDEO_EMBED_MAX_BYTES = 100 * 1024 * 1024;
/** Cores an embed copy may use: a short one is made while the uploader waits. */
export const EMBED_THREADS = 4;
/**
 * Short clips spend the whole size budget on picture: game footage at 30 fps is blocky below
 * ~5 Mbps at 720p. Past this a 720p copy looks no better, it just downloads slower.
 */
const MAX_VIDEO_KBPS = 6000;
/** A long copy stops here: it's watched in a small Discord player, and smaller arrives sooner. */
const LONG_MAX_VIDEO_KBPS = 2500;
/** Below this the picture is too poor to be worth it: the clip gets no copy. */
const MIN_VIDEO_KBPS = 500;
const AUDIO_KBPS = 96;
const MAX_FPS = 30;

export interface EmbedPlan {
  width: number;
  height: number;
  videoKbps: number;
  /** A copy that still came out bigger than this (bitrate overshoot) isn't used. */
  maxBytes: number;
  /** Made by the background worker (a long clip), not while the uploader waits. */
  background: boolean;
}

/** Fits `size` so its shorter side is at most `short`, keeping the aspect ratio and even sides. */
function fit(width: number, height: number, short: number): { width: number; height: number } {
  const even = (n: number) => Math.max(2, Math.round(n / 2) * 2);
  const s = Math.min(width, height);
  if (s <= short) return { width: even(width), height: even(height) };
  const k = short / s;
  return { width: even(width * k), height: even(height * k) };
}

/** The video bitrate that fits `seconds` of clip in `bytes`, keeping 5% for the MP4 container and encoder overshoot. */
function budgetKbps(bytes: number, seconds: number): number {
  return Math.floor(((bytes * 8) / seconds / 1000) * 0.95) - AUDIO_KBPS;
}

/** The embed copy's size and bitrate for a clip, or null when the clip is too long for one. */
export function embedPlan(clip: { width: number; height: number; durationMs: number }): EmbedPlan | null {
  const seconds = Math.max(1, clip.durationMs / 1000);
  let videoKbps = Math.min(MAX_VIDEO_KBPS, budgetKbps(EMBED_TARGET_BYTES, seconds));
  let target = EMBED_TARGET_BYTES;
  if (videoKbps < MIN_VIDEO_KBPS) {
    videoKbps = Math.min(LONG_MAX_VIDEO_KBPS, budgetKbps(LONG_EMBED_TARGET_BYTES, seconds));
    target = LONG_EMBED_TARGET_BYTES;
    if (videoKbps < MIN_VIDEO_KBPS) return null;
  }
  return {
    // A tighter budget looks better at a smaller size.
    ...fit(clip.width, clip.height, videoKbps >= 1500 ? 720 : 540),
    videoKbps,
    maxBytes: Math.round(target * 1.25),
    background: target !== EMBED_TARGET_BYTES,
  };
}

/**
 * Makes `<id>.embed.mp4` from `source`, replacing an older copy in one step. Returns whether there is one now; failures are logged and
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
      width: plan.width,
      height: plan.height,
      videoKbps: plan.videoKbps,
      audioKbps: AUDIO_KBPS,
      fps: info.fps !== null && info.fps > MAX_FPS + 1 ? MAX_FPS : null,
      threads: opts.threads,
    });
    const size = (await stat(tmp)).size;
    if (size > plan.maxBytes) {
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

/** Room one embed copy needs (a long one is up to ~60 MB). */
const EMBED_ROOM_BYTES = 128 * 1024 * 1024;

/**
 * Makes embed copies in the background, one at a time: at start for clips that have none or one
 * made by older settings (EMBED_VERSION), and whenever `kick` says a long upload is waiting for
 * its copy (`embedPending`). Waiting clips go first, then the newest. An old copy is served until
 * its replacement is ready. A failed copy clears `embedPending` (the clip's link is then handed
 * out without one) and is tried again next start.
 */
export class EmbedBackfill {
  private stopped = false;
  private running: Promise<void> | null = null;
  /** A kick came while a pass was running: run another, waiting clips first. */
  private again = false;
  /** Copies that failed since the server started, as `<id>@<mediaVersion>`. */
  private failed = new Set<string>();

  constructor(private opts: { db: Db; clipsDir: string; threads: number; log: FastifyBaseLogger; disk?: DiskGuard }) {}

  start(): void {
    this.kick();
  }

  /** Makes copies for clips waiting for one, soon (after the copy being made now). */
  kick(): void {
    if (this.stopped) return;
    if (this.running) {
      this.again = true;
      return;
    }
    this.running = this.loop();
  }

  /** Resolves once there's nothing left to do (for tests). */
  async idle(): Promise<void> {
    while (this.running) await this.running;
  }

  async stop(): Promise<void> {
    this.stopped = true;
    await this.idle();
  }

  private async loop(): Promise<void> {
    try {
      do {
        this.again = false;
        await this.run();
      } while (this.again && !this.stopped);
    } catch (err) {
      this.opts.log.error({ err }, 'embed backfill crashed');
    } finally {
      this.running = null;
    }
  }

  private async run(): Promise<void> {
    const { db, clipsDir, log } = this.opts;
    const rows = await db
      .select({
        id: clips.id,
        kind: clips.kind,
        mediaVersion: clips.mediaVersion,
        embedVersion: clips.embedVersion,
        embedPending: clips.embedPending,
        width: clips.width,
        height: clips.height,
        durationMs: clips.durationMs,
      })
      .from(clips)
      .where(eq(clips.kind, 'clip'))
      .orderBy(desc(clips.embedPending), desc(clips.createdAt), desc(clips.id));
    let made = 0;
    for (const row of rows) {
      // A new upload waits for its copy: start over so it's next.
      if (this.stopped || this.again) break;
      const key = `${row.id}@${row.mediaVersion}`;
      const plan = embedPlan(row);
      if (!plan || this.failed.has(key)) {
        if (row.embedPending) await this.settle(row, false);
        continue;
      }
      if (!row.embedPending && row.embedVersion >= EMBED_VERSION && (await hasEmbed(clipsDir, row.id))) continue;
      if (this.opts.disk && !(await this.opts.disk.roomFor(EMBED_ROOM_BYTES))) {
        log.warn('embed backfill: stopped, the disk is low on space (tried again next start)');
        // Nobody waits for a copy that won't come: their links go out without one.
        await db.update(clips).set({ embedPending: false }).where(eq(clips.embedPending, true));
        break;
      }
      const source = join(clipsDir, mediaFileName(row));
      if (!(await stat(source).catch(() => null))) continue; // deleted or replaced meanwhile
      const ok = await createEmbed({ clipsDir, clip: row, source, threads: this.opts.threads, log });
      if (!ok) this.failed.add(key);
      if (await this.settle(row, ok)) {
        if (ok) made++;
      } else if (ok && !(await db.select({ id: clips.id }).from(clips).where(eq(clips.id, row.id))).length) {
        await rm(join(clipsDir, embedFileName(row.id)), { force: true }); // deleted while encoding
      }
    }
    if (made > 0) log.info(`embed backfill: made ${made} embed copies`);
  }

  /**
   * Records the outcome for the video the copy was made from; false when the clip was deleted or
   * its video replaced meanwhile (a replacement waits for its own copy, made on the next pass).
   * Compressing a video bumps its version by 1 and replacing it by 2, so +1 is still the same video.
   */
  private async settle(row: { id: string; mediaVersion: number }, made: boolean): Promise<boolean> {
    const [still] = await this.opts.db
      .update(clips)
      .set(made ? { embedVersion: EMBED_VERSION, embedPending: false } : { embedPending: false })
      .where(and(eq(clips.id, row.id), gte(clips.mediaVersion, row.mediaVersion), lte(clips.mediaVersion, row.mediaVersion + 1)))
      .returning({ id: clips.id });
    return !!still;
  }
}
