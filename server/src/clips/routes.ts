import { mkdir, rename, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { and, eq, isNull, lt, or } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import type { Clip } from '@klipt/shared';
import { clips, uploads } from '../db/schema.js';
import fastifyStatic from '@fastify/static';
import { requireUser } from '../auth/plugin.js';
import { isUploadComplete, removeUpload, tusFilePath } from '../uploads/tus.js';
import { assertPlayable, makeThumbnail, MediaError, probeVideo, remuxFaststart } from './media.js';
import { newClipId } from './ids.js';
import { getClip, InvalidCursorError, listClips, listGames } from './repo.js';

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

  const listQuery = z.object({
    scope: z.enum(['feed', 'mine', 'user']).default('feed'),
    username: z.string().max(64).optional(),
    game: z.string().max(100).optional(),
    cursor: z.string().max(512).optional(),
    limit: z.coerce.number().int().min(1).max(60).optional(),
  });
  const gamesQuery = listQuery.pick({ scope: true, username: true });
  const idParam = z.object({ id: z.string().regex(/^[A-Za-z0-9_-]{12}$/) });
  const patchBody = z
    .object({
      title: z.string().trim().min(1).max(100),
      visibility: z.enum(['feed', 'link']),
      gameName: z.string().trim().min(1).max(100).nullable(),
      steamAppId: z.number().int().positive().nullable(),
    })
    .partial()
    .strict();

  app.get('/api/clips', { preHandler: requireUser }, async (req, reply) => {
    const q = listQuery.safeParse(req.query);
    if (!q.success) return reply.code(400).send({ error: 'invalid_request' });
    try {
      const { items, nextCursor } = await listClips(db, req.user!, q.data);
      return { items: items.map((i) => toClip(i.clip, i.owner, config.baseUrl)), nextCursor };
    } catch (err) {
      if (err instanceof InvalidCursorError) return reply.code(400).send({ error: 'invalid_cursor' });
      throw err;
    }
  });

  app.get('/api/games', { preHandler: requireUser }, async (req, reply) => {
    const q = gamesQuery.safeParse(req.query);
    if (!q.success) return reply.code(400).send({ error: 'invalid_request' });
    return listGames(db, req.user!, q.data.scope, q.data.username);
  });

  app.get('/api/clips/:id', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    return toClip(found.clip, found.owner, config.baseUrl);
  });

  app.patch('/api/clips/:id', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    if (found.clip.userId !== req.user!.id) return reply.code(403).send({ error: 'forbidden' });
    const body = patchBody.safeParse(req.body ?? {});
    if (!body.success) return reply.code(400).send({ error: 'invalid_request' });
    const patch = Object.fromEntries(Object.entries(body.data).filter(([, v]) => v !== undefined));
    let row = found.clip;
    if (Object.keys(patch).length > 0) {
      [row] = (await db.update(clips).set(patch).where(eq(clips.id, row.id)).returning()) as [ClipRow];
    }
    return toClip(row, found.owner, config.baseUrl);
  });

  app.delete('/api/clips/:id', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    if (found.clip.userId !== req.user!.id && !req.user!.isAdmin) return reply.code(403).send({ error: 'forbidden' });
    await db.delete(clips).where(eq(clips.id, found.clip.id));
    const dir = join(config.dataDir, 'clips');
    await Promise.all([`${found.clip.id}.mp4`, `${found.clip.id}.jpg`].map((f) => rm(join(dir, f), { force: true })));
    return reply.code(204).send();
  });

  // Media: public (the id is the secret). Constrained filename, no listing, no traversal.
  const clipsDir = join(config.dataDir, 'clips');
  await mkdir(clipsDir, { recursive: true });
  await app.register(fastifyStatic, { root: clipsDir, serve: false });
  app.get('/media/:file', async (req, reply) => {
    const { file } = req.params as { file: string };
    if (!/^[A-Za-z0-9_-]{12}\.(mp4|jpg)$/.test(file)) return reply.code(404).send({ error: 'not_found' });
    if (!(await stat(join(clipsDir, file)).catch(() => null))) return reply.code(404).send({ error: 'not_found' });
    reply.header('cache-control', 'public, max-age=31536000, immutable');
    return reply.sendFile(file, { cacheControl: false });
  });

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
