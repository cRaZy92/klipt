import { mkdir, rename, rm, stat, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { eq, sql } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { ACCENTS, BANNERS, FRAMES, THEMES, sanitizeLooks, unlockedCosmetics, type CosmeticId, type Looks } from '@klipt/shared';
import { requireUser } from '../auth/plugin.js';
import { userAchievements, users } from '../db/schema.js';
import { makeAvatar, probeImage } from '../clips/media.js';
import { MAX_THUMB_BYTES, THUMB_TYPES } from '../clips/routes.js';
import { meOf } from './ref.js';

const AVATAR_FILE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.jpg$/;

const ids = (list: { id: string }[]) => list.map((c) => c.id) as [string, ...string[]];
const looksBody = z
  .object({
    accent: z.enum(ids(ACCENTS)),
    theme: z.enum(ids(THEMES)),
    frame: z.enum(ids(FRAMES)).nullable(),
    banner: z.enum(ids(BANNERS)).nullable(),
  })
  .partial()
  .strict();

/** Your profile picture and looks. Registered after the clip routes (it shares their image body parser). */
export async function userRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;
  const avatarsDir = join(config.dataDir, 'avatars');
  await mkdir(avatarsDir, { recursive: true });

  app.put('/api/me/avatar', { preHandler: requireUser, bodyLimit: MAX_THUMB_BYTES }, async (req, reply) => {
    const type = (req.headers['content-type'] ?? '').split(';')[0]!.trim().toLowerCase();
    if (!THUMB_TYPES.includes(type)) return reply.code(415).send({ error: 'unsupported_media_type', message: 'Use a JPEG, PNG or WebP picture.' });
    if (!Buffer.isBuffer(req.body) || req.body.length === 0) return reply.code(400).send({ error: 'invalid_request' });
    const id = req.user!.id;
    const tmpIn = join(avatarsDir, `.${id}.in.tmp`);
    const tmpOut = join(avatarsDir, `.${id}.jpg.tmp`);
    try {
      await writeFile(tmpIn, req.body);
      if (!(await probeImage(tmpIn))) {
        return reply.code(422).send({ error: 'not_an_image', message: "That file isn't a picture Klipt can read." });
      }
      await makeAvatar(tmpIn, tmpOut);
      await rename(tmpOut, join(avatarsDir, `${id}.jpg`));
    } finally {
      await Promise.all([tmpIn, tmpOut].map((f) => rm(f, { force: true })));
    }
    await db.update(users).set({ avatarVersion: sql`${users.avatarVersion} + 1` }).where(eq(users.id, id));
    return meOf(db, id, config.baseUrl);
  });

  app.delete('/api/me/avatar', { preHandler: requireUser }, async (req) => {
    const id = req.user!.id;
    await db.update(users).set({ avatarVersion: 0 }).where(eq(users.id, id));
    await rm(join(avatarsDir, `${id}.jpg`), { force: true });
    return meOf(db, id, config.baseUrl);
  });

  // Public like clip media: the URL changes with each new picture, so it's cached for good.
  app.get('/avatars/:file', async (req, reply) => {
    const { file } = req.params as { file: string };
    if (!AVATAR_FILE.test(file) || !(await stat(join(avatarsDir, file)).catch(() => null))) {
      return reply.code(404).send({ error: 'not_found' });
    }
    reply.header('cache-control', 'public, max-age=31536000, immutable');
    return reply.sendFile(file, avatarsDir, { cacheControl: false });
  });

  app.put('/api/me/looks', { preHandler: requireUser }, async (req, reply) => {
    const parsed = looksBody.safeParse(req.body);
    if (!parsed.success) return reply.code(400).send({ error: 'invalid_request' });
    const userId = req.user!.id;
    const earned = await db.select({ id: userAchievements.achievementId }).from(userAchievements).where(eq(userAchievements.userId, userId));
    const unlocked = unlockedCosmetics(earned.map((e) => e.id));
    const change = parsed.data;
    for (const kind of ['accent', 'theme', 'frame', 'banner'] as const) {
      const value = change[kind];
      if (value != null && !unlocked.has(`${kind}:${value}` as CosmeticId)) {
        return reply.code(403).send({ error: 'not_unlocked', message: 'Earn the achievement that unlocks it first.' });
      }
    }
    const [row] = await db.select({ looks: users.looks }).from(users).where(eq(users.id, userId));
    const next: Looks = { ...sanitizeLooks(row?.looks), ...change };
    await db.update(users).set({ looks: next }).where(eq(users.id, userId));
    return meOf(db, userId, config.baseUrl);
  });
}
