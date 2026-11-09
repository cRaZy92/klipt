import { randomBytes } from 'node:crypto';
import { and, eq } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import type { DesktopLoginResponse, Me, SessionInfo } from '@klipt/shared';
import { sessions, users } from '../db/schema.js';
import { authRateLimit, requireUser, SESSION_COOKIE } from './plugin.js';
import { hashPassword, verifyPassword } from './passwords.js';
import { createSession, revokeSession } from './sessions.js';

const loginBody = z.object({
  username: z.string().min(1).max(100),
  password: z.string().min(1).max(1024),
  client: z.enum(['web', 'desktop']),
  deviceName: z.string().trim().max(100).optional(),
});

// Real hash, verified against when the user is unknown, to equalize timing.
const dummyHash = hashPassword(randomBytes(16).toString('hex'));

const toMe = (u: { id: string; username: string; isAdmin: boolean }): Me => ({
  id: u.id,
  username: u.username,
  isAdmin: u.isAdmin,
});

export async function authRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;

  app.post('/api/auth/login', { config: { rateLimit: authRateLimit } }, async (req, reply) => {
    const parsed = loginBody.safeParse(req.body);
    if (!parsed.success) return reply.code(400).send({ error: 'invalid_request' });
    const { username, password, client, deviceName } = parsed.data;

    const [user] = await db.select().from(users).where(eq(users.usernameKey, username.toLowerCase())).limit(1);
    const ok = await verifyPassword(user?.passwordHash ?? (await dummyHash), password);
    if (!user || !ok || user.disabledAt) return reply.code(401).send({ error: 'invalid_credentials' });

    const { token } = await createSession(db, user.id, client, deviceName ?? (client === 'web' ? 'Web' : undefined));
    const me = toMe(user);
    if (client === 'desktop') {
      const body: DesktopLoginResponse = { token, me };
      return body;
    }
    reply.setCookie(SESSION_COOKIE, token, {
      httpOnly: true,
      sameSite: 'lax',
      path: '/',
      secure: config.secureCookies,
      maxAge: 30 * 24 * 60 * 60,
    });
    return me;
  });

  app.post('/api/auth/logout', async (req, reply) => {
    if (req.user) await revokeSession(db, req.user.sessionId);
    reply.clearCookie(SESSION_COOKIE, { path: '/', httpOnly: true, sameSite: 'lax', secure: config.secureCookies });
    return reply.code(204).send();
  });

  app.get('/api/me', { preHandler: requireUser }, async (req): Promise<Me> => toMe(req.user!));

  app.get('/api/auth/sessions', { preHandler: requireUser }, async (req): Promise<SessionInfo[]> => {
    const rows = await db.select().from(sessions).where(eq(sessions.userId, req.user!.id)).orderBy(sessions.createdAt);
    return rows.map((s) => ({
      id: s.id,
      kind: s.kind,
      label: s.label,
      createdAt: s.createdAt.toISOString(),
      lastUsedAt: s.lastUsedAt.toISOString(),
      current: s.id === req.user!.sessionId,
    }));
  });

  app.delete('/api/auth/sessions/:id', { preHandler: requireUser }, async (req, reply) => {
    const { id } = req.params as { id: string };
    if (!z.uuid().safeParse(id).success) return reply.code(404).send({ error: 'not_found' });
    const deleted = await db
      .delete(sessions)
      .where(and(eq(sessions.id, id), eq(sessions.userId, req.user!.id)))
      .returning({ id: sessions.id });
    if (deleted.length === 0) return reply.code(404).send({ error: 'not_found' });
    return reply.code(204).send();
  });
}
