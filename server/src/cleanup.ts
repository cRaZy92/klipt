import { and, isNotNull, isNull, lt, or, sql } from 'drizzle-orm';
import type { Db } from './db/client.js';
import { invites, passwordResets, sessions } from './db/schema.js';

export interface ExpiredCleanup {
  sessions: number;
  passwordResets: number;
  invites: number;
}

/**
 * Deletes rows that can never be used again, so the tables don't grow forever:
 * - expired logins, except each user's latest one (the admin page's "last seen" comes from it);
 * - password links past their expiry (used ones too, since nothing reads them after);
 * - invites nobody used that expired or were revoked. Used invites stay: they record who invited whom.
 */
export async function cleanupExpired(db: Db, now: Date): Promise<ExpiredCleanup> {
  const s = await db
    .delete(sessions)
    .where(
      and(
        lt(sessions.expiresAt, now),
        sql`exists (select 1 from ${sessions} newer where newer.user_id = ${sessions.userId}
          and (newer.last_used_at, newer.id) > (${sessions.lastUsedAt}, ${sessions.id}))`,
      ),
    )
    .returning({ id: sessions.id });
  const r = await db.delete(passwordResets).where(lt(passwordResets.expiresAt, now)).returning({ code: passwordResets.code });
  const i = await db
    .delete(invites)
    .where(and(isNull(invites.usedBy), or(lt(invites.expiresAt, now), isNotNull(invites.revokedAt))))
    .returning({ code: invites.code });
  return { sessions: s.length, passwordResets: r.length, invites: i.length };
}
