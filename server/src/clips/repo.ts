import { and, count, desc, eq, ilike, lt, or, sql, type SQL } from 'drizzle-orm';
import type { GameCount, MyStorage } from '@klipt/shared';
import type { AuthUser } from '../auth/sessions.js';
import type { Db } from '../db/client.js';
import { clips, users } from '../db/schema.js';

export type ClipRow = typeof clips.$inferSelect;
export type Scope = 'feed' | 'mine' | 'user';
export interface ClipWithOwner {
  clip: ClipRow;
  owner: { username: string };
}

export class InvalidCursorError extends Error {}

const DEFAULT_LIMIT = 24;
const MAX_LIMIT = 60;

export function encodeCursor(createdAt: Date, id: string): string {
  return Buffer.from(`${createdAt.toISOString()}|${id}`).toString('base64url');
}

export function decodeCursor(cursor: string): { createdAt: Date; id: string } {
  const raw = Buffer.from(cursor, 'base64url').toString('utf8');
  const sep = raw.indexOf('|');
  const iso = raw.slice(0, sep);
  const id = raw.slice(sep + 1);
  const createdAt = new Date(iso);
  if (sep < 0 || !id || Number.isNaN(createdAt.getTime()) || createdAt.toISOString() !== iso) {
    throw new InvalidCursorError('invalid cursor');
  }
  return { createdAt, id };
}

function scopeCondition(viewer: AuthUser, scope: Scope, username?: string): SQL {
  if (scope === 'mine') return eq(clips.userId, viewer.id);
  if (scope === 'feed') return eq(clips.visibility, 'feed');
  const key = (username ?? '').toLowerCase();
  const isSelf = key === viewer.username.toLowerCase();
  const owner = eq(users.usernameKey, key);
  return isSelf ? owner : and(owner, eq(clips.visibility, 'feed'))!;
}

/**
 * Search: every word must appear (case-insensitive) in the title, the game or the uploader's name.
 * Blank text matches everything.
 */
export function searchCondition(text: string | undefined): SQL | undefined {
  const words = (text ?? '').trim().split(/\s+/).filter(Boolean).slice(0, 8);
  if (words.length === 0) return undefined;
  return and(
    ...words.map((word) => {
      const pattern = `%${word.replace(/[\\%_]/g, (c) => `\\${c}`)}%`;
      return or(ilike(clips.title, pattern), ilike(clips.gameName, pattern), ilike(users.username, pattern));
    }),
  );
}

export async function listClips(
  db: Db,
  viewer: AuthUser,
  q: { scope: Scope; username?: string; game?: string; search?: string; cursor?: string; limit?: number },
): Promise<{ items: ClipWithOwner[]; nextCursor: string | null }> {
  const limit = Math.min(Math.max(Math.trunc(q.limit ?? DEFAULT_LIMIT), 1), MAX_LIMIT);
  const conds: (SQL | undefined)[] = [scopeCondition(viewer, q.scope, q.username)];
  if (q.game) conds.push(eq(clips.gameName, q.game));
  conds.push(searchCondition(q.search));
  if (q.cursor) {
    const c = decodeCursor(q.cursor);
    conds.push(or(lt(clips.createdAt, c.createdAt), and(eq(clips.createdAt, c.createdAt), lt(clips.id, c.id))));
  }
  const rows = await db
    .select({ clip: clips, owner: { username: users.username } })
    .from(clips)
    .innerJoin(users, eq(users.id, clips.userId))
    .where(and(...conds))
    .orderBy(desc(clips.createdAt), desc(clips.id))
    .limit(limit + 1);
  const items = rows.slice(0, limit);
  const last = items[items.length - 1];
  const nextCursor = rows.length > limit && last ? encodeCursor(last.clip.createdAt, last.clip.id) : null;
  return { items, nextCursor };
}

export async function listGames(
  db: Db,
  viewer: AuthUser,
  scope: Scope,
  username?: string,
): Promise<GameCount[]> {
  const rows = await db
    .select({
      gameName: clips.gameName,
      steamAppId: sql<number | null>`max(${clips.steamAppId})`,
      count: count(),
    })
    .from(clips)
    .innerJoin(users, eq(users.id, clips.userId))
    .where(and(scopeCondition(viewer, scope, username), sql`${clips.gameName} is not null`))
    .groupBy(clips.gameName)
    .orderBy(desc(count()), clips.gameName);
  return rows.map((r) => ({ gameName: r.gameName!, steamAppId: r.steamAppId ?? null, count: Number(r.count) }));
}

export async function getUserStorage(db: Db, userId: string): Promise<MyStorage> {
  const [row] = await db
    .select({ clips: sql<number>`count(*)::int`, bytes: sql<number>`coalesce(sum(${clips.sizeBytes}), 0)::float8` })
    .from(clips)
    .where(eq(clips.userId, userId));
  return { clips: Number(row!.clips), bytes: Number(row!.bytes) };
}

export async function getClip(db: Db, id: string): Promise<ClipWithOwner | null> {
  const [row] = await db
    .select({ clip: clips, owner: { username: users.username } })
    .from(clips)
    .innerJoin(users, eq(users.id, clips.userId))
    .where(eq(clips.id, id));
  return row ?? null;
}
