import type { ChildProcess } from 'node:child_process';
import { readdir, rename, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { and, desc, eq, inArray, sql } from 'drizzle-orm';
import type { FastifyBaseLogger } from 'fastify';
import type { Db } from '../db/client.js';
import { clips } from '../db/schema.js';
import { MEDIA_FILE, mediaFileName, videoFileName } from './files.js';
import { probeVideo, transcodeH264 } from './media.js';

type ClipRow = typeof clips.$inferSelect;

export interface TranscoderOptions {
  db: Db;
  clipsDir: string;
  log: FastifyBaseLogger;
  enabled: boolean;
  crf: number;
  preset: string;
  /** CPU threads one compression may use. */
  threads: number;
  /** How long a replaced file is kept for viewers who are mid-play (ms). */
  oldFileGraceMs?: number;
}

/** The job ffmpeg is on right now. */
export interface CurrentJob {
  clipId: string;
  /** 0..1 */
  progress: number;
  startedAt: Date;
}

/** Keep the compressed file only when it's at least this much smaller than the upload. */
const KEEP_RATIO = 0.9;

/**
 * Compresses uploaded clips in the background, one at a time, newest first (so a fresh upload
 * doesn't wait behind the backlog). The queue is the `clips.transcode_state` column, so it survives
 * restarts. A clip plays from its upload until its smaller file is ready; the swap is a new file
 * name, so browsers never mix bytes of the two files from their (immutable) cache.
 */
export class Transcoder {
  private running: Promise<void> | null = null;
  private wanted = false;
  private stopped = false;
  private child: ChildProcess | null = null;
  private current: CurrentJob | null = null;
  /** Deleted mid-job: the job must not write its result back. */
  private cancelled = new Set<string>();
  private timers = new Set<NodeJS.Timeout>();

  constructor(private opts: TranscoderOptions) {}

  get enabled(): boolean {
    return this.opts.enabled;
  }

  /** Recovers from a restart (an interrupted job goes back in the queue, leftover files go) and starts working. */
  async start(): Promise<void> {
    if (!this.opts.enabled) {
      // Turned off: nothing will compress them, so clips mustn't show as waiting.
      await this.opts.db
        .update(clips)
        .set({ transcodeState: null, transcodeStartedAt: null })
        .where(inArray(clips.transcodeState, ['queued', 'running']));
      return;
    }
    await this.opts.db.update(clips).set({ transcodeState: 'queued', transcodeStartedAt: null }).where(eq(clips.transcodeState, 'running'));
    await this.sweepOrphans().catch((err) => this.opts.log.error({ err }, 'transcode: orphan sweep failed'));
    this.kick();
  }

  /** There may be new work: start the worker unless it's already going. */
  kick(): void {
    if (!this.opts.enabled || this.stopped) return;
    this.wanted = true;
    if (this.running) return;
    this.running = this.loop()
      .catch((err) => this.opts.log.error({ err }, 'transcode worker crashed'))
      .finally(() => {
        this.running = null;
      });
  }

  /** The clip was deleted: stop compressing it. */
  cancel(clipId: string): void {
    if (this.current?.clipId !== clipId) return;
    this.cancelled.add(clipId);
    this.child?.kill('SIGKILL');
  }

  status(): CurrentJob | null {
    return this.current ? { ...this.current } : null;
  }

  /** Resolves once the queue is empty (for tests). */
  async idle(): Promise<void> {
    while (this.running) await this.running;
  }

  async stop(): Promise<void> {
    this.stopped = true;
    this.child?.kill('SIGKILL');
    await this.running;
    for (const t of this.timers) clearTimeout(t);
    this.timers.clear();
  }

  private async loop(): Promise<void> {
    while (!this.stopped) {
      this.wanted = false;
      const job = await this.claimNext();
      if (!job) {
        if (this.wanted) continue;
        return;
      }
      await this.process(job);
    }
  }

  private async claimNext(): Promise<ClipRow | null> {
    const { db } = this.opts;
    for (;;) {
      const [next] = await db
        .select({ id: clips.id })
        .from(clips)
        .where(eq(clips.transcodeState, 'queued'))
        .orderBy(desc(clips.createdAt), desc(clips.id))
        .limit(1);
      if (!next) return null;
      const [claimed] = await db
        .update(clips)
        .set({ transcodeState: 'running', transcodeStartedAt: new Date(), transcodeFinishedAt: null, transcodeError: null })
        .where(and(eq(clips.id, next.id), eq(clips.transcodeState, 'queued')))
        .returning();
      if (claimed) return claimed;
    }
  }

  private async process(job: ClipRow): Promise<void> {
    const { db, clipsDir, log } = this.opts;
    const source = join(clipsDir, mediaFileName(job));
    const version = job.mediaVersion + 1;
    const outName = videoFileName(job.id, version);
    const out = join(clipsDir, outName);
    const tmp = join(clipsDir, `.${outName}.tmp`);
    this.current = { clipId: job.id, progress: 0, startedAt: job.transcodeStartedAt ?? new Date() };
    const finish = (set: Partial<typeof clips.$inferInsert>) =>
      db
        .update(clips)
        .set({ ...set, transcodeFinishedAt: new Date() })
        .where(and(eq(clips.id, job.id), eq(clips.transcodeState, 'running')))
        .returning({ id: clips.id });
    try {
      const originalSize = (await stat(source)).size;
      await transcodeH264(source, tmp, {
        crf: this.opts.crf,
        preset: this.opts.preset,
        threads: this.opts.threads,
        durationMs: job.durationMs,
        onSpawn: (child) => (this.child = child),
        onProgress: (p) => {
          if (this.current) this.current.progress = p;
        },
      });
      this.child = null;
      if (this.cancelled.has(job.id)) throw new Error('cancelled');
      // A sanity check before the upload is thrown away: the result must be a full-length H.264 video.
      const info = await probeVideo(tmp);
      if (info.videoCodec !== 'h264' || Math.abs(info.durationMs - job.durationMs) > 1500) {
        throw new Error(`compressed file looks wrong (codec ${info.videoCodec}, ${info.durationMs} ms of ${job.durationMs} ms)`);
      }
      const newSize = (await stat(tmp)).size;
      const originalBytes = job.originalBytes ?? originalSize;
      if (newSize > originalSize * KEEP_RATIO) {
        await rm(tmp, { force: true });
        await finish({ transcodeState: 'skipped', originalBytes });
        log.info({ clipId: job.id, originalSize, newSize }, 'transcode: kept the upload (barely smaller)');
        return;
      }
      await rename(tmp, out);
      const saved = await finish({ transcodeState: 'done', mediaVersion: version, sizeBytes: newSize, originalBytes });
      if (saved.length === 0) {
        // Deleted (or reset) meanwhile.
        await rm(out, { force: true });
        return;
      }
      this.removeLater(source);
      log.info({ clipId: job.id, originalSize, newSize }, 'transcode: done');
    } catch (err) {
      this.child = null;
      await rm(tmp, { force: true });
      if (this.cancelled.has(job.id)) return;
      if (this.stopped) {
        // Shutting down: the job goes back in the queue on the next start.
        return;
      }
      const message = err instanceof Error ? err.message : String(err);
      log.error({ err, clipId: job.id }, 'transcode failed');
      await finish({ transcodeState: 'failed', transcodeError: message.slice(-1000) }).catch(() => {});
    } finally {
      this.cancelled.delete(job.id);
      this.current = null;
    }
  }

  /** Viewers who started the clip before the swap keep streaming the old file for a while. */
  private removeLater(path: string): void {
    const t = setTimeout(() => {
      this.timers.delete(t);
      rm(path, { force: true }).catch(() => {});
    }, this.opts.oldFileGraceMs ?? 10 * 60 * 1000);
    t.unref();
    this.timers.add(t);
  }

  /**
   * Removes video files no clip points at: a replaced upload whose delayed removal a restart cut
   * short, a result whose database write never happened, and half-written temp files.
   */
  private async sweepOrphans(): Promise<void> {
    const { db, clipsDir, log } = this.opts;
    const names = await readdir(clipsDir).catch(() => [] as string[]);
    const videos = names.filter((n) => n.endsWith('.mp4') && MEDIA_FILE.test(n));
    const temps = names.filter((n) => /^\.[A-Za-z0-9_-]{12}\.v\d+\.mp4\.tmp$/.test(n));
    const ids = [...new Set(videos.map((n) => n.slice(0, 12)))];
    const rows = ids.length
      ? await db
          .select({ id: clips.id, kind: clips.kind, mediaVersion: clips.mediaVersion })
          .from(clips)
          .where(inArray(clips.id, ids))
      : [];
    const live = new Set(rows.map((r) => mediaFileName(r)));
    const known = new Set(rows.map((r) => r.id));
    // Only clips that exist: a file without a row may be an upload whose finalize is still running.
    const orphans = videos.filter((n) => known.has(n.slice(0, 12)) && !live.has(n));
    for (const n of [...orphans, ...temps]) await rm(join(clipsDir, n), { force: true });
    if (orphans.length + temps.length > 0) log.info(`transcode: removed ${orphans.length + temps.length} leftover files`);
  }
}

export interface TranscodeCounts {
  queued: number;
  running: number;
  done: number;
  skipped: number;
  failed: number;
  /** Space compression has freed so far. */
  savedBytes: number;
}

export async function transcodeCounts(db: Db): Promise<TranscodeCounts> {
  const rows = await db
    .select({
      state: clips.transcodeState,
      n: sql<number>`count(*)::int`,
      saved: sql<number>`coalesce(sum(${clips.originalBytes} - ${clips.sizeBytes}), 0)::float8`,
    })
    .from(clips)
    .where(sql`${clips.transcodeState} is not null`)
    .groupBy(clips.transcodeState);
  const counts: TranscodeCounts = { queued: 0, running: 0, done: 0, skipped: 0, failed: 0, savedBytes: 0 };
  for (const r of rows) {
    counts[r.state!] = Number(r.n);
    if (r.state === 'done') counts.savedBytes = Number(r.saved);
  }
  return counts;
}
