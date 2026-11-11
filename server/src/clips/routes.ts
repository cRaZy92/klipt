import { mkdir, rename, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { and, eq, isNull, lt, or } from 'drizzle-orm';
import type { FastifyInstance, FastifyReply } from 'fastify';
import { z } from 'zod';
import type { Clip } from '@klipt/shared';
import { clips, uploads } from '../db/schema.js';
import fastifyStatic from '@fastify/static';
import { requireUser } from '../auth/plugin.js';
import { isUploadComplete, removeTusFiles, removeUpload, tusFilePath } from '../uploads/tus.js';
import { assertPlayable, makeThumbnail, MediaError, probeImage, probeVideo, remuxFaststart, toPng } from './media.js';
import { newClipId } from './ids.js';
import {
  getClip,
  getUserStorage,
  InvalidCursorError,
  listClips,
  listGames,
  listUsernames,
  listViewers,
  recordView,
  resolveTags,
  setTags,
  UnknownUserError,
} from './repo.js';

type ClipRow = typeof clips.$inferSelect;

/** Maps a clip row to its API shape. Shared by every clip endpoint. */
export function toClip(row: ClipRow, owner: { username: string }, baseUrl: string, views = 0, tagged: string[] = []): Clip {
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
    views,
    tagged,
    kind: row.kind,
    createdAt: row.createdAt.toISOString(),
    owner: { username: owner.username },
    shareUrl: `${baseUrl}/c/${row.id}`,
    videoUrl: `${baseUrl}/media/${row.id}.${row.kind === 'screenshot' ? 'png' : 'mp4'}`,
    thumbnailUrl: `${baseUrl}/media/${row.id}.jpg`,
  };
}

const CLAIM_TTL_MS = 30 * 60 * 1000;

class AlreadyFinalizedError extends Error {}

const finalizeBody = z.object({
  uploadId: z.string().min(1).max(128),
  title: z.string().trim().min(1).max(100),
  gameName: z.string().trim().min(1).max(100).nullish(),
  steamAppId: z.number().int().positive().nullish(),
  visibility: z.enum(['feed', 'link']).optional(),
  /** The frame (ms into the uploaded video) to use as the thumbnail. */
  thumbnailMs: z.number().int().min(0).max(24 * 60 * 60 * 1000).nullish(),
  /** Friends in the clip, by username. */
  tagged: z.array(z.string().max(64)).max(20).optional(),
});

/**
 * Content-Disposition for saving a clip as "<title>.mp4": characters that file systems reject
 * become "_", with a plain-ASCII filename for old clients and the full UTF-8 name in filename*.
 */
export function downloadDisposition(title: string, ext: 'mp4' | 'png' = 'mp4'): string {
  const name = title.replace(/[\u0000-\u001f\u007f/\\:*?"<>|]/g, '_').trim().slice(0, 100) || 'klipt-clip';
  const ascii = name.replace(/[^\x20-\x7e]/g, '_');
  const utf8 = encodeURIComponent(name).replace(/['()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
  return `attachment; filename="${ascii}.${ext}"; filename*=UTF-8''${utf8}.${ext}`;
}

const mediaMessages: Record<string, string> = {
  not_a_video: "That file isn't a video Klipt can play.",
  unsupported_codec: 'Klipt can only play H.264 video with AAC audio (or no audio).',
  corrupt_video: "That video file is damaged and can't be played.",
};

export async function clipRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;

  const listQuery = z.object({
    scope: z.enum(['feed', 'mine', 'user']).default('feed'),
    username: z.string().max(64).optional(),
    game: z.string().max(100).optional(),
    q: z.string().max(100).optional(),
    /** `me`: only clips you're tagged in. */
    with: z.enum(['me']).optional(),
    /** Only video clips or only screenshots. */
    kind: z.enum(['clip', 'screenshot']).optional(),
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
      tagged: z.array(z.string().max(64)).max(20),
    })
    .partial()
    .strict();

  const unknownUser = (reply: FastifyReply, err: UnknownUserError) =>
    reply.code(400).send({ error: 'unknown_user', message: err.message });

  app.get('/api/users', { preHandler: requireUser }, async () => listUsernames(db));

  app.get('/api/clips', { preHandler: requireUser }, async (req, reply) => {
    const q = listQuery.safeParse(req.query);
    if (!q.success) return reply.code(400).send({ error: 'invalid_request' });
    try {
      const { items, nextCursor } = await listClips(db, req.user!, { ...q.data, search: q.data.q, withMe: q.data.with === 'me', kind: q.data.kind });
      return { items: items.map((i) => toClip(i.clip, i.owner, config.baseUrl, i.views, i.tagged)), nextCursor };
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

  app.get('/api/me/storage', { preHandler: requireUser }, async (req) => getUserStorage(db, req.user!.id));

  app.get('/api/clips/:id', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    return toClip(found.clip, found.owner, config.baseUrl, found.views, found.tagged);
  });

  // A play of the clip, from the website or the app. Open to logged-out visitors (share links),
  // so it's rate limited per address.
  app.post('/api/clips/:id/view', { config: { rateLimit: { max: 60, timeWindow: '1 minute' } } }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    if (!p.success || !(await recordView(db, p.data.id, req.user?.id ?? null))) return reply.code(404).send({ error: 'not_found' });
    return reply.code(204).send();
  });

  app.get('/api/clips/:id/viewers', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    if (found.clip.userId !== req.user!.id) return reply.code(403).send({ error: 'forbidden' });
    return listViewers(db, found.clip.id);
  });

  app.patch('/api/clips/:id', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    if (found.clip.userId !== req.user!.id) return reply.code(403).send({ error: 'forbidden' });
    const body = patchBody.safeParse(req.body ?? {});
    if (!body.success) return reply.code(400).send({ error: 'invalid_request' });
    const { tagged, ...fields } = body.data;
    if (tagged !== undefined) {
      let ids: string[];
      try {
        ids = await resolveTags(db, found.clip.userId, tagged);
      } catch (err) {
        if (err instanceof UnknownUserError) return unknownUser(reply, err);
        throw err;
      }
      await db.transaction((tx) => setTags(tx, found.clip.id, ids));
    }
    const patch = Object.fromEntries(Object.entries(fields).filter(([, v]) => v !== undefined));
    if (Object.keys(patch).length > 0) await db.update(clips).set(patch).where(eq(clips.id, found.clip.id));
    const now = (await getClip(db, found.clip.id))!;
    return toClip(now.clip, now.owner, config.baseUrl, now.views, now.tagged);
  });

  app.delete('/api/clips/:id', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    if (found.clip.userId !== req.user!.id && !req.user!.isAdmin) return reply.code(403).send({ error: 'forbidden' });
    await db.delete(clips).where(eq(clips.id, found.clip.id));
    const dir = join(config.dataDir, 'clips');
    await Promise.all(['mp4', 'jpg', 'png'].map((ext) => rm(join(dir, `${found.clip.id}.${ext}`), { force: true })));
    return reply.code(204).send();
  });

  // Media: public (the id is the secret). Constrained filename, no listing, no traversal.
  const clipsDir = join(config.dataDir, 'clips');
  await mkdir(clipsDir, { recursive: true });
  await app.register(fastifyStatic, { root: clipsDir, serve: false });
  app.get('/media/:file', async (req, reply) => {
    const { file } = req.params as { file: string };
    if (!/^[A-Za-z0-9_-]{12}\.(mp4|jpg|png)$/.test(file)) return reply.code(404).send({ error: 'not_found' });
    if (!(await stat(join(clipsDir, file)).catch(() => null))) return reply.code(404).send({ error: 'not_found' });
    if (!file.endsWith('.jpg') && (req.query as { download?: string }).download === '1') {
      const found = await getClip(db, file.slice(0, 12));
      if (!found) return reply.code(404).send({ error: 'not_found' });
      reply.header('content-disposition', downloadDisposition(found.clip.title, file.endsWith('.png') ? 'png' : 'mp4'));
    }
    reply.header('cache-control', 'public, max-age=31536000, immutable');
    return reply.sendFile(file, { cacheControl: false });
  });

  app.post('/api/clips', { preHandler: requireUser }, async (req, reply) => {
    const parsed = finalizeBody.safeParse(req.body);
    if (!parsed.success) return reply.code(400).send({ error: 'invalid_request' });
    const body = parsed.data;
    const user = req.user!;

    const ownedUpload = async () => {
      const [row] = await db
        .select({ id: uploads.id, clipId: uploads.clipId })
        .from(uploads)
        .where(and(eq(uploads.id, body.uploadId), eq(uploads.userId, user.id)));
      return row;
    };
    // A finalized upload is kept as a tombstone, so a client that lost the 201 can retry and learn its clip.
    const sendFinalized = async (clipId: string) => {
      const found = await getClip(db, clipId);
      if (!found) return reply.code(404).send({ error: 'not_found' });
      return reply.code(200).send(toClip(found.clip, found.owner, config.baseUrl, found.views, found.tagged));
    };

    const owned = await ownedUpload();
    if (!owned) return reply.code(404).send({ error: 'not_found' });
    if (owned.clipId) return sendFinalized(owned.clipId);
    let tagIds: string[];
    try {
      tagIds = await resolveTags(db, user.id, body.tagged ?? []);
    } catch (err) {
      if (err instanceof UnknownUserError) return unknownUser(reply, err);
      throw err;
    }

    // Atomically claim the upload so concurrent/retried finalizes can't both process it.
    const [upload] = await db
      .update(uploads)
      .set({ finalizingAt: new Date() })
      .where(
        and(
          eq(uploads.id, owned.id),
          isNull(uploads.clipId),
          or(isNull(uploads.finalizingAt), lt(uploads.finalizingAt, new Date(Date.now() - CLAIM_TTL_MS))),
        ),
      )
      .returning();
    if (!upload) {
      const now = await ownedUpload();
      if (now?.clipId) return sendFinalized(now.clipId);
      if (!now) return reply.code(404).send({ error: 'not_found' });
      return reply.code(409).send({ error: 'upload_finalizing' });
    }
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
    const imagePath = join(clipsDir, `${id}.png`);
    const tmpImage = join(clipsDir, `.${id}.png.tmp`);
    const cleanOutputs = () =>
      Promise.all([videoPath, thumbPath, tmpVideo, tmpThumb, imagePath, tmpImage].map((p) => rm(p, { force: true })));

    let row: ClipRow;
    try {
      const source = tusFilePath(config.dataDir, upload.id);
      // A screenshot is kept as a PNG; anything else must be a playable video.
      const image = await probeImage(source);
      let info: { durationMs: number; width: number; height: number };
      let size: number;
      if (image) {
        info = { durationMs: 0, ...image };
        await toPng(source, tmpImage);
        await makeThumbnail(tmpImage, tmpThumb, 0);
        size = (await stat(tmpImage)).size;
        await rename(tmpImage, imagePath);
      } else {
        info = await probeVideo(source);
        assertPlayable(info as Awaited<ReturnType<typeof probeVideo>>);
        await remuxFaststart(source, tmpVideo);
        await makeThumbnail(tmpVideo, tmpThumb, info.durationMs, body.thumbnailMs);
        size = (await stat(tmpVideo)).size;
        await rename(tmpVideo, videoPath);
      }
      await rename(tmpThumb, thumbPath);
      // Insert the clip and turn the upload into its tombstone in one step; if another finalize
      // (one that took over a stale claim) got there first, roll back and answer with its clip.
      row = await db.transaction(async (tx) => {
        const [inserted] = (await tx
          .insert(clips)
          .values({
            id,
            userId: user.id,
            title: body.title,
            gameName: body.gameName ?? null,
            steamAppId: body.steamAppId ?? null,
            visibility: body.visibility ?? 'feed',
            kind: image ? 'screenshot' : 'clip',
            durationMs: info.durationMs,
            width: info.width,
            height: info.height,
            sizeBytes: size,
          })
          .returning()) as [ClipRow];
        const tomb = await tx
          .update(uploads)
          .set({ clipId: id, finalizingAt: new Date() })
          .where(and(eq(uploads.id, upload.id), isNull(uploads.clipId)))
          .returning({ id: uploads.id });
        if (tomb.length === 0) throw new AlreadyFinalizedError();
        await setTags(tx, id, tagIds);
        return inserted;
      });
    } catch (err) {
      // The insert is transactional, so outputs here are never committed media.
      await cleanOutputs();
      if (err instanceof AlreadyFinalizedError) {
        const now = await ownedUpload();
        if (now?.clipId) return sendFinalized(now.clipId);
        return reply.code(404).send({ error: 'not_found' });
      }
      if (err instanceof MediaError) {
        await removeUpload(db, config.dataDir, upload.id).catch((e) => req.log.error(e, 'upload cleanup failed'));
        return reply.code(422).send({ error: err.code, message: mediaMessages[err.code] ?? err.message });
      }
      await release();
      throw err;
    }
    // The clip is committed; tus file removal is best-effort (stale cleanup will catch leftovers).
    await removeTusFiles(config.dataDir, upload.id).catch((e) => req.log.error(e, 'upload cleanup failed'));
    const created = await getClip(db, row.id);
    return reply.code(201).send(toClip(row, user, config.baseUrl, 0, created?.tagged ?? []));
  });
}
