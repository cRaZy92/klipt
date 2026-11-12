import { randomBytes } from 'node:crypto';
import { and, eq } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import { nanoid } from 'nanoid';
import { z } from 'zod';
import type { DesktopLoginResponse, Me, SessionInfo } from '@klipt/shared';
import { shareInviter } from '../clips/repo.js';
import { invites, sessions, users } from '../db/schema.js';
import { authRateLimit, requireUser, SESSION_COOKIE, setSessionCookie } from './plugin.js';
import { hashPassword, verifyPassword } from './passwords.js';
import { activeInvite } from '../admin/queries.js';
import { createSession, listSessions, revokeSession } from './sessions.js';

const registerBody = z
  .object({
    /** An invite link an admin made. */
    inviteCode: z.string().min(1).max(100).optional(),
    /** Or a clip's share link: its owner is the inviter. */
    clipId: z.string().regex(/^[A-Za-z0-9_-]{12}$/).optional(),
    username: z.string(),
    password: z.string().max(1024),
  })
  .refine((b) => (b.inviteCode === undefined) !== (b.clipId === undefined));
const USERNAME_RE = /^[A-Za-z0-9_]{3,24}$/;

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
    setSessionCookie(reply, token, config);
    return me;
  });

  app.post('/api/register', { config: { rateLimit: authRateLimit } }, async (req, reply) => {
    const parsed = registerBody.safeParse(req.body);
    if (!parsed.success) return reply.code(400).send({ error: 'invalid_request' });
    const { inviteCode, clipId, username, password } = parsed.data;
    if (!USERNAME_RE.test(username)) return reply.code(400).send({ error: 'invalid_username' });
    if (password.length < 8) return reply.code(400).send({ error: 'weak_password' });
    const passwordHash = await hashPassword(password);

    // Rolling back the transaction (via throw) keeps a failed registration from consuming the invite.
    class Fail extends Error {
      constructor(readonly status: number, readonly code: string) {
        super(code);
      }
    }
    let user: typeof users.$inferSelect;
    try {
      user = await db.transaction(async (tx) => {
        const now = new Date();
        let inviter: string | null = null;
        if (inviteCode !== undefined) {
          const claimed = await tx
            .update(invites)
            .set({ usedAt: now })
            .where(and(eq(invites.code, inviteCode), activeInvite(now)))
            .returning({ code: invites.code });
          if (claimed.length === 0) throw new Fail(400, 'invalid_invite');
        } else {
          inviter = config.shareSignup ? await shareInviter(tx, clipId!) : null;
          if (!inviter) throw new Fail(400, 'invalid_share');
        }
        const inserted = await tx
          .insert(users)
          .values({ username, usernameKey: username.toLowerCase(), passwordHash })
          .onConflictDoNothing({ target: users.usernameKey })
          .returning();
        if (inserted.length === 0) throw new Fail(409, 'username_taken');
        const created = inserted[0]!;
        if (inviteCode !== undefined) {
          await tx.update(invites).set({ usedBy: created.id }).where(eq(invites.code, inviteCode));
        } else {
          // Kept like an invite from the clip's owner, used on the spot: every account has an inviter.
          await tx.insert(invites).values({ code: nanoid(16), createdBy: inviter!, expiresAt: now, usedAt: now, usedBy: created.id });
        }
        return created;
      });
    } catch (e) {
      if (e instanceof Fail) return reply.code(e.status).send({ error: e.code });
      throw e;
    }

    const { token } = await createSession(db, user.id, 'web', 'Web');
    setSessionCookie(reply, token, config);
    return toMe(user);
  });

  app.post('/api/auth/logout', async (req, reply) => {
    if (req.user) await revokeSession(db, req.user.sessionId);
    reply.clearCookie(SESSION_COOKIE, { path: '/', httpOnly: true, sameSite: 'lax', secure: config.secureCookies });
    return reply.code(204).send();
  });

  app.get('/api/me', { preHandler: requireUser }, async (req): Promise<Me> => toMe(req.user!));

  app.get('/api/auth/sessions', { preHandler: requireUser }, async (req): Promise<SessionInfo[]> =>
    listSessions(db, req.user!),
  );

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
