import type { FastifyInstance, FastifyReply } from 'fastify';
import { z } from 'zod';
import { MAX_ALBUM_CLIPS, type Album } from '@klipt/shared';
import { requireUser } from '../auth/plugin.js';
import { toClip } from '../clips/routes.js';
import {
  addAlbumClip,
  AlbumFullError,
  albumClipList,
  createAlbum,
  deleteAlbum,
  getAlbum,
  listAlbums,
  removeAlbumClip,
  renameAlbum,
  reorderAlbum,
  toAlbumSummary,
  UnknownClipError,
  type AlbumWithOwner,
} from './repo.js';

const ID = /^[A-Za-z0-9_-]{12}$/;
const clipId = z.string().regex(ID);
const title = z.string().trim().min(1).max(100);

export async function albumRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;
  const idParam = z.object({ id: z.string().regex(ID) });
  const clipParam = z.object({ id: z.string().regex(ID), clipId });
  const createBody = z.object({ title, clipIds: z.array(clipId).max(MAX_ALBUM_CLIPS).optional() }).strict();
  const patchBody = z.object({ title, clipIds: z.array(clipId).max(MAX_ALBUM_CLIPS) }).partial().strict();

  const full = (reply: FastifyReply) =>
    reply.code(409).send({ error: 'album_full', message: `An album holds up to ${MAX_ALBUM_CLIPS} clips.` });
  const unknownClip = (reply: FastifyReply) => reply.code(400).send({ error: 'unknown_clip' });

  const withClips = async (a: AlbumWithOwner, viewerId: string): Promise<Album> => ({
    ...toAlbumSummary(a, config.baseUrl),
    clips: (await albumClipList(db, a.album.id, viewerId)).map((c) => toClip(c, config.baseUrl)),
  });

  /**
   * The album in `params` when the logged-in user may change it (its owner), else a 404 or 403
   * sent and null. Admins may only delete one.
   */
  async function owned(params: unknown, reply: FastifyReply, userId: string, adminOk = false): Promise<AlbumWithOwner | null> {
    const p = idParam.safeParse(params);
    const found = p.success ? await getAlbum(db, p.data.id) : null;
    if (!found) {
      await reply.code(404).send({ error: 'not_found' });
      return null;
    }
    if (found.album.userId !== userId && !adminOk) {
      await reply.code(403).send({ error: 'forbidden' });
      return null;
    }
    return found;
  }

  // Your albums; `?clip=<id>` says for each whether it holds that clip (the "Add to album" list).
  app.get('/api/albums', { preHandler: requireUser }, async (req, reply) => {
    const q = z.object({ clip: clipId.optional() }).safeParse(req.query);
    if (!q.success) return reply.code(400).send({ error: 'invalid_request' });
    return (await listAlbums(db, req.user!.id, q.data.clip)).map((a) => toAlbumSummary(a, config.baseUrl));
  });

  app.post('/api/albums', { preHandler: requireUser }, async (req, reply) => {
    const body = createBody.safeParse(req.body ?? {});
    if (!body.success) return reply.code(400).send({ error: 'invalid_request' });
    try {
      const id = await createAlbum(db, req.user!.id, body.data.title, body.data.clipIds);
      return reply.code(201).send(await withClips((await getAlbum(db, id))!, req.user!.id));
    } catch (err) {
      if (err instanceof AlbumFullError) return full(reply);
      if (err instanceof UnknownClipError) return unknownClip(reply);
      throw err;
    }
  });

  // Any album you have the id of, like a clip's share link.
  app.get('/api/albums/:id', { preHandler: requireUser }, async (req, reply) => {
    const p = idParam.safeParse(req.params);
    const found = p.success ? await getAlbum(db, p.data.id) : null;
    if (!found) return reply.code(404).send({ error: 'not_found' });
    return withClips(found, req.user!.id);
  });

  // Rename, or put the clips in a new order (`clipIds` must be exactly the clips it holds).
  app.patch('/api/albums/:id', { preHandler: requireUser }, async (req, reply) => {
    const found = await owned(req.params, reply, req.user!.id);
    if (!found) return reply;
    const body = patchBody.safeParse(req.body ?? {});
    if (!body.success) return reply.code(400).send({ error: 'invalid_request' });
    if (body.data.clipIds && !(await reorderAlbum(db, found.album.id, body.data.clipIds))) {
      return reply.code(409).send({ error: 'album_changed' });
    }
    if (body.data.title !== undefined) await renameAlbum(db, found.album.id, body.data.title);
    return withClips((await getAlbum(db, found.album.id))!, req.user!.id);
  });

  app.delete('/api/albums/:id', { preHandler: requireUser }, async (req, reply) => {
    const found = await owned(req.params, reply, req.user!.id, req.user!.isAdmin);
    if (!found) return reply;
    await deleteAlbum(db, found.album.id);
    return reply.code(204).send();
  });

  // Adds a clip at the end (any clip you can open: the id is the secret), or takes it out.
  for (const method of ['PUT', 'DELETE'] as const) {
    app.route({
      method,
      url: '/api/albums/:id/clips/:clipId',
      preHandler: requireUser,
      handler: async (req, reply) => {
        const p = clipParam.safeParse(req.params);
        if (!p.success) return reply.code(404).send({ error: 'not_found' });
        const found = await owned({ id: p.data.id }, reply, req.user!.id);
        if (!found) return reply;
        try {
          if (method === 'PUT') await addAlbumClip(db, found.album.id, p.data.clipId);
          else await removeAlbumClip(db, found.album.id, p.data.clipId);
        } catch (err) {
          if (err instanceof AlbumFullError) return full(reply);
          if (err instanceof UnknownClipError) return reply.code(404).send({ error: 'not_found' });
          throw err;
        }
        return reply.code(204).send();
      },
    });
  }
}
