import type { SessionInfo } from '@klipt/shared';
import { createHash, randomBytes } from 'node:crypto';
import { and, eq, gt, isNull, lt, or, sql } from 'drizzle-orm';
import type { Db } from '../db/client.js';
import { sessions, users } from '../db/schema.js';

export type SessionRow = typeof sessions.$inferSelect;
export interface AuthUser {
  id: string;
  username: string;
  isAdmin: boolean;
  sessionId: string;
}

const WEB_TTL_MS = 30 * 24 * 60 * 60 * 1000;
const LAST_USED_GRANULARITY_MS = 60 * 1000;

export const hashToken = (token: string): string => createHash('sha256').update(token).digest('hex');

export async function createSession(
  db: Db,
  userId: string,
  kind: 'web' | 'desktop',
  label?: string,
): Promise<{ token: string; session: SessionRow }> {
  const token = randomBytes(32).toString('base64url');
  const [session] = await db
    .insert(sessions)
    .values({
      userId,
      kind,
      label: label ?? null,
      tokenHash: hashToken(token),
      expiresAt: kind === 'web' ? new Date(Date.now() + WEB_TTL_MS) : null,
    })
    .returning();
  return { token, session: session! };
}

export async function resolveToken(db: Db, token: string): Promise<AuthUser | null> {
  if (!token || token.length > 256) return null;
  const now = new Date();
  const [row] = await db
    .select({
      sessionId: sessions.id,
      lastUsedAt: sessions.lastUsedAt,
      id: users.id,
      username: users.username,
      isAdmin: users.isAdmin,
    })
    .from(sessions)
    .innerJoin(users, eq(users.id, sessions.userId))
    .where(
      and(
        eq(sessions.tokenHash, hashToken(token)),
        or(isNull(sessions.expiresAt), gt(sessions.expiresAt, now)),
        isNull(users.disabledAt),
      ),
    )
    .limit(1);
  if (!row) return null;
  if (now.getTime() - row.lastUsedAt.getTime() >= LAST_USED_GRANULARITY_MS) {
    await db
      .update(sessions)
      .set({ lastUsedAt: now })
      .where(and(eq(sessions.id, row.sessionId), lt(sessions.lastUsedAt, sql`${new Date(now.getTime() - LAST_USED_GRANULARITY_MS + 1)}`)));
  }
  return { id: row.id, username: row.username, isAdmin: row.isAdmin, sessionId: row.sessionId };
}

export async function revokeSession(db: Db, sessionId: string): Promise<void> {
  await db.delete(sessions).where(eq(sessions.id, sessionId));
}

export async function revokeAllForUser(db: Db, userId: string): Promise<void> {
  await db.delete(sessions).where(eq(sessions.userId, userId));
}

export async function listSessions(db: Db, user: AuthUser): Promise<SessionInfo[]> {
  const rows = await db.select().from(sessions).where(eq(sessions.userId, user.id)).orderBy(sessions.createdAt);
  return rows.map((s) => ({
    id: s.id,
    kind: s.kind,
    label: s.label,
    createdAt: s.createdAt.toISOString(),
    lastUsedAt: s.lastUsedAt.toISOString(),
    current: s.id === user.sessionId,
  }));
}
