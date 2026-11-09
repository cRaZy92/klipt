import { mkdir, rename, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { and, eq, isNull, lt, or } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import type { Clip } from '@klipt/shared';
import { clips, uploads } from '../db/schema.js';
import { requireUser } from '../auth/plugin.js';
import { isUploadComplete, removeUpload, tusFilePath } from '../uploads/tus.js';
import { assertPlayable, makeThumbnail, MediaError, probeVideo, remuxFaststart } from './media.js';
import { newClipId } from './ids.js';

type ClipRow = typeof clips.$inferSelect;

/** Maps a clip row to its API shape. Shared by every clip endpoint. */
export function toClip(row: ClipRow, owner: { username: string }, baseUrl: string): Clip {
  return {
    id: row.id,
    title: row.title,
    gameName: row.gameName,
    steamAppId: row.steamAppId,
    visibility: row.visibility,
    durationMs: row.durationMs,
    width: row.width,
    height: row.height,
    sizeBytes: row.sizeBytes,
    createdAt: row.createdAt.toISOString(),
    owner: { username: owner.username },
    shareUrl: `${baseUrl}/c/${row.id}`,
    videoUrl: `${baseUrl}/media/${row.id}.mp4`,
    thumbnailUrl: `${baseUrl}/media/${row.id}.jpg`,
  };
}

const CLAIM_TTL_MS = 30 * 60 * 1000;

const finalizeBody = z.object({
  uploadId: z.string().min(1).max(128),
  title: z.string().trim().min(1).max(100),
  gameName: z.string().trim().min(1).max(100).nullish(),
  steamAppId: z.number().int().positive().nullish(),
  visibility: z.enum(['feed', 'link']).optional(),
});

const mediaMessages: Record<string, string> = {
  not_a_video: "That file isn't a video Klipt can play.",
  unsupported_codec: 'Klipt can only play H.264 video with AAC audio (or no audio).',
};

export async function clipRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;

  app.post('/api/clips', { preHandler: requireUser }, async (req, reply) => {
    const parsed = finalizeBody.safeParse(req.body);
    if (!parsed.success) return reply.code(400).send({ error: 'invalid_request' });
    const body = parsed.data;
    const user = req.user!;

    const [owned] = await db
      .select({ id: uploads.id })
      .from(uploads)
      .where(and(eq(uploads.id, body.uploadId), eq(uploads.userId, user.id)));
    if (!owned) return reply.code(404).send({ error: 'not_found' });

    // Atomically claim the upload so concurrent/retried finalizes can't both process it.
    const [upload] = await db
      .update(uploads)
      .set({ finalizingAt: new Date() })
      .where(
        and(
          eq(uploads.id, owned.id),
          or(isNull(uploads.finalizingAt), lt(uploads.finalizingAt, new Date(Date.now() - CLAIM_TTL_MS))),
        ),
      )
      .returning();
    if (!upload) return reply.code(409).send({ error: 'upload_finalizing' });
    const release = () =>
      db.update(uploads).set({ finalizingAt: null }).where(eq(uploads.id, upload.id)).catch(() => {});

    if (!(await isUploadComplete(config.dataDir, upload.id))) {
      await release();
      return reply.code(409).send({ error: 'upload_incomplete' });
    }

    const id = newClipId();
    const clipsDir = join(config.dataDir, 'clips');
    await mkdir(clipsDir, { recursive: true });
    const videoPath = join(clipsDir, `${id}.mp4`);
    const thumbPath = join(clipsDir, `${id}.jpg`);
    const tmpVideo = join(clipsDir, `.${id}.mp4.tmp`);
    const tmpThumb = join(clipsDir, `.${id}.jpg.tmp`);
    const cleanOutputs = () =>
      Promise.all([videoPath, thumbPath, tmpVideo, tmpThumb].map((p) => rm(p, { force: true })));

    let row: ClipRow;
    try {
      const source = tusFilePath(config.dataDir, upload.id);
      const info = await probeVideo(source);
      assertPlayable(info);
      await remuxFaststart(source, tmpVideo);
      await makeThumbnail(tmpVideo, tmpThumb, info.durationMs);
      const { size } = await stat(tmpVideo);
      await rename(tmpVideo, videoPath);
      await rename(tmpThumb, thumbPath);
      [row] = await db
        .insert(clips)
        .values({
          id,
          userId: user.id,
          title: body.title,
          gameName: body.gameName ?? null,
          steamAppId: body.steamAppId ?? null,
          visibility: body.visibility ?? 'feed',
          durationMs: info.durationMs,
          width: info.width,
          height: info.height,
          sizeBytes: size,
        })
        .returning() as [ClipRow];
    } catch (err) {
      // Only failures before the insert reach here, so outputs are never committed media.
      await cleanOutputs();
      if (err instanceof MediaError) {
        await removeUpload(db, config.dataDir, upload.id).catch((e) => req.log.error(e, 'upload cleanup failed'));
        return reply.code(422).send({ error: err.code, message: mediaMessages[err.code] ?? err.message });
      }
      await release();
      throw err;
    }
    // The clip is committed; upload removal is best-effort (stale cleanup will catch leftovers).
    await removeUpload(db, config.dataDir, upload.id).catch((e) => req.log.error(e, 'upload cleanup failed'));
    return reply.code(201).send(toClip(row, user, config.baseUrl));
  });
}
