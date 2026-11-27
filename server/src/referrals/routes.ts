import { and, asc, eq, inArray, isNull, sql } from 'drizzle-orm';
import type { FastifyInstance, FastifyReply } from 'fastify';
import { nanoid } from 'nanoid';
import type { Referral } from '@klipt/shared';
import { requireUser } from '../auth/plugin.js';
import { activeInvite } from '../admin/queries.js';
import type { Db } from '../db/client.js';
import { invites, users } from '../db/schema.js';
import { makeFriends } from '../friends/repo.js';
import { displayNameOf } from '../users/ref.js';
import { ReferralPage } from '../web/pages/referral.js';

/** How long a referral link works. */
export const REFERRAL_DAYS = 14;
/** Holds the code of the referral link a logged-out visitor came from, until they sign up. */
export const REF_COOKIE = 'klipt_ref';
const CODE = /^[A-Za-z0-9_-]{1,100}$/;

export const referralUrl = (baseUrl: string, code: string) => `${baseUrl}/r/${code}`;

/** A code that works: unused, not revoked or expired, and its sender is still active. */
const usable = (code: string) =>
  and(eq(invites.code, code), activeInvite(), inArray(invites.createdBy, sql`(select ${users.id} from ${users} where ${users.disabledAt} is null)`))!;

/**
 * Uses `code` for `userId` and makes them friends with its sender, in `tx`. Returns the sender, or
 * null when the code doesn't work (then nothing changes). Never the user's own code.
 */
export async function claimReferral(tx: Pick<Db, 'update' | 'insert'>, code: string, userId: string): Promise<string | null> {
  if (!CODE.test(code)) return null;
  const [claimed] = await tx
    .update(invites)
    .set({ usedAt: new Date(), usedBy: userId })
    .where(and(usable(code), sql`${invites.createdBy} <> ${userId}::uuid`))
    .returning({ createdBy: invites.createdBy });
  if (!claimed) return null;
  await makeFriends(tx, claimed.createdBy, userId);
  return claimed.createdBy;
}

/** The sender of a working `code`, for "X invited you". */
export async function referrerOf(db: Db, code: string | undefined): Promise<{ id: string; username: string; displayName: string } | null> {
  if (!code || !CODE.test(code)) return null;
  const [row] = await db
    .select({ id: users.id, username: users.username, displayName: displayNameOf(users) })
    .from(invites)
    .innerJoin(users, eq(users.id, invites.createdBy))
    .where(usable(code));
  return row ?? null;
}

/** `userId`'s links that still work, oldest first. */
export async function listReferrals(db: Db, userId: string, baseUrl: string): Promise<Referral[]> {
  const rows = await db
    .select()
    .from(invites)
    .where(and(eq(invites.createdBy, userId), activeInvite()))
    .orderBy(asc(invites.createdAt));
  return rows.map((r) => ({ code: r.code, url: referralUrl(baseUrl, r.code), expiresAt: r.expiresAt.toISOString() }));
}

export function clearRefCookie(reply: FastifyReply, secure: boolean): void {
  reply.clearCookie(REF_COOKIE, { path: '/', httpOnly: true, sameSite: 'lax', secure });
}

export async function referralRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;
  const toReferral = (r: { code: string; expiresAt: Date }): Referral => ({
    code: r.code,
    url: referralUrl(config.baseUrl, r.code),
    expiresAt: r.expiresAt.toISOString(),
  });

  app.post('/api/referrals', { preHandler: requireUser }, async (req): Promise<Referral> => {
    const [row] = await db
      .insert(invites)
      .values({ code: nanoid(16), createdBy: req.user!.id, expiresAt: new Date(Date.now() + REFERRAL_DAYS * 24 * 60 * 60 * 1000) })
      .returning();
    return toReferral(row!);
  });

  app.get('/api/referrals', { preHandler: requireUser }, async (req): Promise<Referral[]> => listReferrals(db, req.user!.id, config.baseUrl));

  app.delete('/api/referrals/:code', { preHandler: requireUser }, async (req, reply) => {
    const { code } = req.params as { code: string };
    const res = await db
      .update(invites)
      .set({ revokedAt: new Date() })
      .where(and(eq(invites.code, code), eq(invites.createdBy, req.user!.id), isNull(invites.usedBy), isNull(invites.revokedAt)))
      .returning({ code: invites.code });
    if (res.length === 0) return reply.code(404).send({ error: 'not_found' });
    return reply.code(204).send();
  });

  // The link people send. Logged out: remember it for sign-up. Logged in: become friends now.
  app.get('/r/:code', async (req, reply) => {
    const { code } = req.params as { code: string };
    const user = req.user;
    if (!user) {
      if (!(await referrerOf(db, code))) return reply.redirect('/?ref=invalid');
      reply.setCookie(REF_COOKIE, code, {
        httpOnly: true,
        sameSite: 'lax',
        path: '/',
        secure: config.secureCookies,
        maxAge: REFERRAL_DAYS * 24 * 60 * 60,
      });
      return reply.redirect('/?ref=1');
    }
    // Logged in: ask first, so another site can't make you someone's friend just by sending you here.
    const sender = await referrerOf(db, code);
    if (!sender) return reply.redirect('/friends?ref=invalid');
    if (sender.id === user.id) return reply.redirect('/friends?ref=own');
    return reply.type('text/html; charset=utf-8').header('cache-control', 'private, no-cache').send(ReferralPage({ user, code, sender }) as string);
  });

  app.post('/api/referrals/:code/accept', { preHandler: requireUser }, async (req, reply) => {
    const { code } = req.params as { code: string };
    const sender = await referrerOf(db, code);
    const claimed = sender && sender.id !== req.user!.id ? await db.transaction((tx) => claimReferral(tx, code, req.user!.id)) : null;
    if (!claimed) return reply.code(400).send({ error: 'invalid_referral', message: 'That link was already used or expired.' });
    return { username: sender!.username };
  });

  // Links made before referrals (admin invites) work the same way.
  app.get('/invite/:code', async (req, reply) => {
    const { code } = req.params as { code: string };
    return reply.redirect(`/r/${encodeURIComponent(code)}`);
  });
}
