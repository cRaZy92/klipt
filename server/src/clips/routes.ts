import { mkdir, rename, rm, stat, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { and, eq, isNull, lt, or, sql } from 'drizzle-orm';
import type { FastifyInstance, FastifyReply } from 'fastify';
import { z } from 'zod';
import type { Clip } from '@klipt/shared';
import { clips, uploads } from '../db/schema.js';
import fastifyStatic from '@fastify/static';
import { requireUser } from '../auth/plugin.js';
import { isUploadComplete, removeTusFiles, removeUpload, tusFilePath } from '../uploads/tus.js';
import { assertPlayable, makeThumbnail, MediaError, probeImage, probeVideo, remuxFaststart, toPng } from './media.js';
import { newClipId } from './ids.js';
import { embedFileName, MEDIA_FILE, mediaFileName, videoFileName } from './files.js';
import { createEmbed, EMBED_VERSION } from './embed.js';
import {
  getClip,
  getUserStorage,
  setFavorite,
  InvalidCursorError,
  listClips,
  listGames,
  listUsernames,
  listViewers,
  recordView,
  resolveTags,
  setTags,
  UnknownUserError,
  type ClipWithOwner,
} from './repo.js';

type ClipRow = typeof clips.$inferSelect;

/** Maps a clip row to its API shape. Shared by every clip endpoint. */
export function toClip(item: ClipWithOwner, baseUrl: string): Clip {
  const { clip: row, owner, views, tagged, taggedNames, favorite } = item;
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
    taggedNames,
    favorite,
    kind: row.kind,
    processing: processingOf(row),
    createdAt: row.createdAt.toISOString(),
    owner: { username: owner.username, displayName: owner.displayName },
    shareUrl: `${baseUrl}/c/${row.id}`,
    videoUrl: `${baseUrl}/media/${mediaFileName(row)}`,
    thumbnailUrl: thumbnailUrl(row, baseUrl),
  };
}

/**
 * `?v=<n>` once a clip's video was replaced: the thumbnail and embed copy keep their names but
 * change with the video, and the thumbnail is cached as immutable.
 */
export function versionQuery(row: Pick<ClipRow, 'mediaVersion'>): string {
  return row.mediaVersion > 0 ? `?v=${row.mediaVersion}` : '';
}

/**
 * `?v=` for the thumbnail: the video's version, plus `-<n>` once the owner set it from an image
 * (the thumbnail is cached as immutable, so each new picture needs a new URL).
 */
export function thumbQuery(row: Pick<ClipRow, 'mediaVersion' | 'thumbVersion'>): string {
  return row.thumbVersion > 0 ? `?v=${row.mediaVersion}-${row.thumbVersion}` : versionQuery(row);
}

/** A clip's thumbnail URL (`baseUrl` '' gives a path on this site). */
export function thumbnailUrl(row: Pick<ClipRow, 'id' | 'mediaVersion' | 'thumbVersion'>, baseUrl: string): string {
  return `${baseUrl}/media/${row.id}.jpg${thumbQuery(row)}`;
}

/** Whether the server is still compressing the clip (it plays meanwhile). */
export function processingOf(row: Pick<ClipRow, 'transcodeState'>): 'queued' | 'running' | null {
  return row.transcodeState === 'queued' || row.transcodeState === 'running' ? row.transcodeState : null;
}

/** Pictures the app may send as a thumbnail, and how big. */
const THUMB_TYPES = ['image/jpeg', 'image/png', 'image/webp'];
const MAX_THUMB_BYTES = 10 * 1024 * 1024;

const CLAIM_TTL_MS = 30 * 60 * 1000;
/** The embed copy is made while the uploader waits (a few seconds), so it gets more cores than compression. */
const EMBED_THREADS = 4;

class AlreadyFinalizedError extends Error {}
/** The clip an upload was to replace is gone (deleted meanwhile). */
class ReplaceTargetGoneError extends Error {}

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
  /**
   * One of your video clips whose video this upload replaces: same link, views, stars and post
   * date. Its tags stay unless `tagged` is sent.
   */
  replaces: z.string().regex(/^[A-Za-z0-9_-]{12}$/).optional(),
  /**
   * `high` (admins only): the video is kept as uploaded, never compressed, for edits and
   * compilations that the usual compression would make blocky.
   */
  quality: z.enum(['standard', 'high']).optional(),
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
    /** `1`: only clips you starred. */
    favorites: z.enum(['1']).optional(),
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
      const { items, nextCursor } = await listClips(db, req.user!, { ...q.data, search: q.data.q, withMe: q.data.with === 'me', kind: q.data.kind, favorites: q.data.favorites === '1' });
      return { items: items.map((i) => toClip(i, config.baseUrl)), nextCursor };
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
    const found = p.success ? await getClip(db, p.data.id, req.user?.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    return toClip(found, config.baseUrl);
  });

  // Starring: any clip you can open (the id is the secret, as for the share page).
  for (const [method, favorite] of [['PUT', true], ['DELETE', false]] as const) {
    app.route({
      method,
      url: '/api/clips/:id/favorite',
      preHandler: requireUser,
      handler: async (req, reply) => {
        const p = idParam.safeParse(req.params);
        if (!p.success || !(await setFavorite(db, p.data.id, req.user!.id, favorite))) return reply.code(404).send({ error: 'not_found' });
        return reply.code(204).send();
      },
    });
  }

  // A play of the clip, from the website or the app. Open to logged-out visitors (share links),
  // so it's rate limited per address.
  app.post('/api/clips/:id/view', { config: { rateLimit: { max: 60, timeWindow: '1 minute' } } }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    if (!p.success || !(await recordView(db, p.data.id, req.user?.id ?? null))) return reply.code(404).send({ error: 'not_found' });
    return reply.code(204).send();
  });

  app.get('/api/clips/:id/viewers', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id, req.user?.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    if (found.clip.userId !== req.user!.id) return reply.code(403).send({ error: 'forbidden' });
    return listViewers(db, found.clip.id);
  });

  app.patch('/api/clips/:id', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id, req.user?.id) : null;
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
    const now = (await getClip(db, found.clip.id, req.user!.id))!;
    return toClip(now, config.baseUrl);
  });

  // A thumbnail picked by the owner: the picture's bytes as the body.
  app.addContentTypeParser(THUMB_TYPES, { parseAs: 'buffer', bodyLimit: MAX_THUMB_BYTES }, (_req, body, done) => done(null, body));
  app.put('/api/clips/:id/thumbnail', { preHandler: requireUser, bodyLimit: MAX_THUMB_BYTES }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id, req.user?.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    if (found.clip.userId !== req.user!.id) return reply.code(403).send({ error: 'forbidden' });
    if (found.clip.kind !== 'clip') {
      return reply.code(400).send({ error: 'invalid_request', message: "A screenshot's thumbnail is its picture." });
    }
    if (!Buffer.isBuffer(req.body) || req.body.length === 0) return reply.code(400).send({ error: 'invalid_request' });
    const id = found.clip.id;
    const dir = join(config.dataDir, 'clips');
    const tmpIn = join(dir, `.${id}.thumb-in.tmp`);
    const tmpThumb = join(dir, `.${id}.thumb.jpg.tmp`);
    try {
      await writeFile(tmpIn, req.body);
      if (!(await probeImage(tmpIn))) {
        return reply.code(422).send({ error: 'not_an_image', message: "That file isn't a picture Klipt can read." });
      }
      await makeThumbnail(tmpIn, tmpThumb, 0);
      await rename(tmpThumb, join(dir, `${id}.jpg`));
    } finally {
      await Promise.all([tmpIn, tmpThumb].map((f) => rm(f, { force: true })));
    }
    await db.update(clips).set({ thumbVersion: sql`${clips.thumbVersion} + 1` }).where(eq(clips.id, id));
    const now = (await getClip(db, id, req.user!.id))!;
    return toClip(now, config.baseUrl);
  });

  app.delete('/api/clips/:id', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getClip(db, p.data.id, req.user?.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    if (found.clip.userId !== req.user!.id && !req.user!.isAdmin) return reply.code(403).send({ error: 'forbidden' });
    await db.delete(clips).where(eq(clips.id, found.clip.id));
    app.transcoder.cancel(found.clip.id);
    const dir = join(config.dataDir, 'clips');
    // The upload too: a compressed clip keeps it for a few minutes after the swap.
    const files = new Set([mediaFileName(found.clip), `${found.clip.id}.mp4`, `${found.clip.id}.jpg`, embedFileName(found.clip.id)]);
    await Promise.all([...files].map((f) => rm(join(dir, f), { force: true })));
    return reply.code(204).send();
  });

  // Media: public (the id is the secret). Constrained filename, no listing, no traversal.
  const clipsDir = join(config.dataDir, 'clips');
  await mkdir(clipsDir, { recursive: true });
  await app.register(fastifyStatic, { root: clipsDir, serve: false });
  const versionOf = (file: string) => Number(/\.v(\d+)\.mp4$/.exec(file)?.[1] ?? 0);
  /** The clip's current video when `file` names an earlier version of it, else null. */
  const currentVideoFor = async (file: string): Promise<string | null> => {
    const found = await getClip(db, file.slice(0, 12));
    if (!found || found.clip.kind !== 'clip' || file.endsWith('.embed.mp4') || versionOf(file) >= found.clip.mediaVersion) return null;
    const current = mediaFileName(found.clip);
    return (await stat(join(clipsDir, current)).catch(() => null)) ? current : null;
  };
  app.get('/media/:file', async (req, reply) => {
    const { file } = req.params as { file: string };
    if (!MEDIA_FILE.test(file)) return reply.code(404).send({ error: 'not_found' });
    if (!(await stat(join(clipsDir, file)).catch(() => null))) {
      // An older name of a clip's video (compression replaced it): serve the current one. Discord
      // keeps the og:video URL it saw when the link was posted, often before compression finished.
      const current = file.endsWith('.mp4') ? await currentVideoFor(file) : null;
      if (!current) return reply.code(404).send({ error: 'not_found' });
      reply.header('cache-control', 'public, max-age=3600');
      return reply.sendFile(current, { cacheControl: false });
    }
    if (!file.endsWith('.jpg') && (req.query as { download?: string }).download === '1') {
      const found = await getClip(db, file.slice(0, 12));
      if (!found) return reply.code(404).send({ error: 'not_found' });
      reply.header('content-disposition', downloadDisposition(found.clip.title, file.endsWith('.png') ? 'png' : 'mp4'));
    }
    // A video or picture never changes under its name; an embed copy is remade in place when the
    // embed settings change (EMBED_VERSION), so caches must come back for it.
    const immutable = !file.endsWith('.embed.mp4');
    reply.header('cache-control', immutable ? 'public, max-age=31536000, immutable' : 'public, max-age=3600');
    return reply.sendFile(file, { cacheControl: false });
  });

  app.post('/api/clips', { preHandler: requireUser }, async (req, reply) => {
    const parsed = finalizeBody.safeParse(req.body);
    if (!parsed.success) return reply.code(400).send({ error: 'invalid_request' });
    const body = parsed.data;
    const user = req.user!;
    const highQuality = body.quality === 'high';
    if (highQuality && !user.isAdmin) {
      return reply.code(403).send({ error: 'forbidden', message: 'Only admins can upload in high quality.' });
    }

    const ownedUpload = async () => {
      const [row] = await db
        .select({ id: uploads.id, clipId: uploads.clipId })
        .from(uploads)
        .where(and(eq(uploads.id, body.uploadId), eq(uploads.userId, user.id)));
      return row;
    };
    // A finalized upload is kept as a tombstone, so a client that lost the 201 can retry and learn its clip.
    const sendFinalized = async (clipId: string) => {
      const found = await getClip(db, clipId, user.id);
      if (!found) return reply.code(404).send({ error: 'not_found' });
      return reply.code(200).send(toClip(found, config.baseUrl));
    };

    const owned = await ownedUpload();
    if (!owned) return reply.code(404).send({ error: 'not_found' });
    if (owned.clipId) return sendFinalized(owned.clipId);
    // `replace_missing`: the client finalizes the same upload as a new clip instead.
    const replaceMissing = () =>
      reply.code(404).send({ error: 'replace_missing', message: 'The clip to replace is no longer on the server.' });
    let target: ClipRow | null = null;
    if (body.replaces) {
      const found = await getClip(db, body.replaces);
      if (!found || found.clip.userId !== user.id) return replaceMissing();
      if (found.clip.kind !== 'clip') {
        return reply.code(400).send({ error: 'invalid_request', message: "A screenshot's picture can't be replaced." });
      }
      target = found.clip;
    }
    // Null: a replaced clip keeps its tags.
    let tagIds: string[] | null;
    try {
      tagIds = target && body.tagged === undefined ? null : await resolveTags(db, user.id, body.tagged ?? []);
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

    const id = target?.id ?? newClipId();
    const clipsDir = join(config.dataDir, 'clips');
    await mkdir(clipsDir, { recursive: true });
    // A replacement is a new version of the video, so no cache mixes the two files. +2, not +1:
    // a compression still running on the old video writes version +1 (and is then thrown away).
    const version = target ? target.mediaVersion + 2 : 0;
    const videoPath = join(clipsDir, videoFileName(id, version));
    const thumbPath = join(clipsDir, `${id}.jpg`);
    const tmpVideo = join(clipsDir, `.${id}.mp4.tmp`);
    const tmpThumb = join(clipsDir, `.${id}.jpg.tmp`);
    const imagePath = join(clipsDir, `${id}.png`);
    const tmpImage = join(clipsDir, `.${id}.png.tmp`);
    const embedPath = join(clipsDir, embedFileName(id));
    // A replaced clip's thumbnail and embed copy are live files: only what this request made goes.
    const outputs = target ? [videoPath, tmpVideo, tmpThumb] : [videoPath, thumbPath, tmpVideo, tmpThumb, imagePath, tmpImage, embedPath];
    const cleanOutputs = () => Promise.all(outputs.map((p) => rm(p, { force: true })));

    let row: ClipRow;
    let embedMade = false;
    try {
      const source = tusFilePath(config.dataDir, upload.id);
      // A screenshot is kept as a PNG; anything else must be a playable video.
      const image = await probeImage(source);
      let info: { durationMs: number; width: number; height: number };
      let size: number;
      if (image && target) throw new MediaError('not_a_video', 'a clip can only be replaced by a video');
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
        // Made now, not queued: the share link is handed out the moment this request answers.
        embedMade = await createEmbed({ clipsDir, clip: { id, ...info }, source: tmpVideo, threads: EMBED_THREADS, log: req.log });
        await rename(tmpVideo, videoPath);
      }
      // A replaced clip keeps its old thumbnail until the swap is committed.
      if (!target) await rename(tmpThumb, thumbPath);
      // Insert the clip and turn the upload into its tombstone in one step; if another finalize
      // (one that took over a stale claim) got there first, roll back and answer with its clip.
      const media = {
        title: body.title,
        gameName: body.gameName ?? null,
        steamAppId: body.steamAppId ?? null,
        transcodeState: !image && !highQuality && config.transcode.enabled ? ('queued' as const) : null,
        highQuality: !image && highQuality,
        durationMs: info.durationMs,
        width: info.width,
        height: info.height,
        sizeBytes: size,
        embedVersion: embedMade ? EMBED_VERSION : 0,
      };
      row = await db.transaction(async (tx) => {
        const [inserted] = target
          ? ((await tx
              .update(clips)
              .set({
                ...media,
                ...(body.visibility ? { visibility: body.visibility } : {}),
                mediaVersion: version,
                originalBytes: null,
                transcodeError: null,
                transcodeStartedAt: null,
                transcodeFinishedAt: null,
              })
              .where(and(eq(clips.id, id), eq(clips.userId, user.id)))
              .returning()) as ClipRow[])
          : ((await tx
              .insert(clips)
              .values({ ...media, id, userId: user.id, visibility: body.visibility ?? 'feed', kind: image ? 'screenshot' : 'clip' })
              .returning()) as ClipRow[]);
        if (!inserted) throw new ReplaceTargetGoneError();
        const tomb = await tx
          .update(uploads)
          .set({ clipId: id, finalizingAt: new Date() })
          .where(and(eq(uploads.id, upload.id), isNull(uploads.clipId)))
          .returning({ id: uploads.id });
        if (tomb.length === 0) throw new AlreadyFinalizedError();
        if (tagIds) await setTags(tx, id, tagIds);
        return inserted;
      });
    } catch (err) {
      // The insert is transactional, so outputs here are never committed media.
      await cleanOutputs();
      if (err instanceof ReplaceTargetGoneError) {
        await release();
        return replaceMissing();
      }
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
    if (target) {
      // A compression of the old video is pointless now (its result wouldn't be saved anyway).
      app.transcoder.cancel(id);
      await rename(tmpThumb, thumbPath).catch((e) => req.log.error(e, 'replacing the thumbnail failed'));
      // An old embed copy shows the old video: none is better (the embed then uses the video).
      if (!embedMade) await rm(embedPath, { force: true });
      // Earlier video files go once viewers who are mid-play are done with them.
      app.transcoder.removeLater(...Array.from({ length: version }, (_, v) => join(clipsDir, videoFileName(id, v))));
    }
    if (row.transcodeState === 'queued') app.transcoder.kick();
    const created = await getClip(db, row.id, user.id);
    if (target && created) {
      return reply.code(200).send(toClip(created, config.baseUrl));
    }
    const item = created ?? {
      clip: row,
      owner: { username: user.username, displayName: user.displayName },
      views: 0,
      tagged: [],
      taggedNames: [],
      favorite: false,
    };
    return reply.code(201).send(toClip(item, config.baseUrl));
  });
}
