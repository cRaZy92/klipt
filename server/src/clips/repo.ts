import { and, asc, count, desc, eq, ilike, inArray, isNull, lt, or, sql, type SQL } from 'drizzle-orm';
import type { GameCount, MyStorage } from '@klipt/shared';
import type { AuthUser } from '../auth/sessions.js';
import type { Db } from '../db/client.js';
import { clipTags, clipViews, clips, users } from '../db/schema.js';

export type ClipRow = typeof clips.$inferSelect;
export type Scope = 'feed' | 'mine' | 'user';
export interface ClipWithOwner {
  clip: ClipRow;
  owner: { username: string };
  /** Friends who watched it plus logged-out plays. */
  views: number;
  /** Friends in the clip, by name. */
  tagged: string[];
}

/** The usernames tagged in a clip, alphabetically. */
const taggedExpr = sql<string[]>`coalesce((select array_agg(tu.username order by tu.username_key) from ${clipTags} ct join ${users} tu on tu.id = ct.user_id where ct.clip_id = ${clips.id}), '{}')`;

/** Clips `username` is tagged in. */
function taggedIn(usernameKey: string): SQL {
  return sql`exists (select 1 from ${clipTags} ct join ${users} tu on tu.id = ct.user_id where ct.clip_id = ${clips.id} and tu.username_key = ${usernameKey})`;
}

/** A clip's view count: its logged-in viewers plus its logged-out plays. */
const viewsExpr = sql<number>`(${clips.anonViews} + (select count(*) from ${clipViews} where ${clipViews.clipId} = ${clips.id}))::int`;

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

/**
 * Which clips a list shows. A profile (`user`) has the user's own clips (link-only ones only to
 * themselves) and, unless `ownOnly`, feed clips they're tagged in.
 */
function scopeCondition(viewer: AuthUser, scope: Scope, username?: string, ownOnly = false): SQL {
  if (scope === 'mine') return eq(clips.userId, viewer.id);
  if (scope === 'feed') return eq(clips.visibility, 'feed');
  const key = (username ?? '').toLowerCase();
  const isSelf = key === viewer.username.toLowerCase();
  const owner = eq(users.usernameKey, key);
  const own = isSelf ? owner : and(owner, eq(clips.visibility, 'feed'))!;
  return ownOnly ? own : or(own, and(eq(clips.visibility, 'feed'), taggedIn(key)))!;
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
  q: { scope: Scope; username?: string; game?: string; search?: string; withMe?: boolean; kind?: 'clip' | 'screenshot'; cursor?: string; limit?: number },
): Promise<{ items: ClipWithOwner[]; nextCursor: string | null }> {
  const limit = Math.min(Math.max(Math.trunc(q.limit ?? DEFAULT_LIMIT), 1), MAX_LIMIT);
  const conds: (SQL | undefined)[] = [scopeCondition(viewer, q.scope, q.username)];
  if (q.game) conds.push(eq(clips.gameName, q.game));
  conds.push(searchCondition(q.search));
  if (q.withMe) conds.push(taggedIn(viewer.username.toLowerCase()));
  if (q.kind) conds.push(eq(clips.kind, q.kind));
  if (q.cursor) {
    const c = decodeCursor(q.cursor);
    conds.push(or(lt(clips.createdAt, c.createdAt), and(eq(clips.createdAt, c.createdAt), lt(clips.id, c.id))));
  }
  const rows = await db
    .select({ clip: clips, owner: { username: users.username }, views: viewsExpr, tagged: taggedExpr })
    .from(clips)
    .innerJoin(users, eq(users.id, clips.userId))
    .where(and(...conds))
    .orderBy(desc(clips.createdAt), desc(clips.id))
    .limit(limit + 1);
  const items = rows.slice(0, limit).map((r) => ({ ...r, views: Number(r.views), tagged: r.tagged ?? [] }));
  const last = items[items.length - 1];
  const nextCursor = rows.length > limit && last ? encodeCursor(last.clip.createdAt, last.clip.id) : null;
  return { items, nextCursor };
}

export interface ProfileStats {
  clips: number;
  views: number;
  /** The clip with the most views; null when none has any. */
  top: { id: string; title: string; views: number } | null;
}

/** A profile's totals over the clips `viewer` may see there (feed clips, or all for your own). */
export async function getProfileStats(db: Db, viewer: AuthUser, username: string): Promise<ProfileStats> {
  const where = scopeCondition(viewer, 'user', username, true);
  const [totals] = await db
    .select({ clips: sql<number>`count(*)::int`, views: sql<number>`coalesce(sum(${viewsExpr}), 0)::int` })
    .from(clips)
    .innerJoin(users, eq(users.id, clips.userId))
    .where(where);
  const [top] = await db
    .select({ id: clips.id, title: clips.title, views: viewsExpr })
    .from(clips)
    .innerJoin(users, eq(users.id, clips.userId))
    .where(where)
    .orderBy(desc(viewsExpr), desc(clips.createdAt))
    .limit(1);
  return {
    clips: Number(totals!.clips),
    views: Number(totals!.views),
    top: top && Number(top.views) > 0 ? { id: top.id, title: top.title, views: Number(top.views) } : null,
  };
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

/**
 * Counts a play of `clipId`: a logged-in viewer once (never the owner), a logged-out one every
 * time. False when the clip doesn't exist.
 */
export async function recordView(db: Db, clipId: string, viewerId: string | null): Promise<boolean> {
  const [clip] = await db.select({ userId: clips.userId }).from(clips).where(eq(clips.id, clipId));
  if (!clip) return false;
  if (viewerId === null) {
    await db.update(clips).set({ anonViews: sql`${clips.anonViews} + 1` }).where(eq(clips.id, clipId));
  } else if (viewerId !== clip.userId) {
    await db.insert(clipViews).values({ clipId, userId: viewerId }).onConflictDoNothing();
  }
  return true;
}

/** The friends who watched `clipId`, first viewer first. */
export async function listViewers(db: Db, clipId: string): Promise<{ username: string; viewedAt: string }[]> {
  const rows = await db
    .select({ username: users.username, viewedAt: clipViews.viewedAt })
    .from(clipViews)
    .innerJoin(users, eq(users.id, clipViews.userId))
    .where(eq(clipViews.clipId, clipId))
    .orderBy(clipViews.viewedAt, users.username);
  return rows.map((r) => ({ username: r.username, viewedAt: r.viewedAt.toISOString() }));
}

export class UnknownUserError extends Error {
  constructor(public username: string) {
    super(`No user called ${username}.`);
  }
}

/**
 * The user ids for tagging `names` in a clip of `ownerId`: case-insensitive, duplicates and the
 * owner dropped. Throws UnknownUserError for a name that isn't an active user.
 */
export async function resolveTags(db: Db, ownerId: string, names: string[]): Promise<string[]> {
  const keys = [...new Set(names.map((n) => n.trim().toLowerCase()).filter(Boolean))];
  if (keys.length === 0) return [];
  const rows = await db
    .select({ id: users.id, key: users.usernameKey })
    .from(users)
    .where(and(inArray(users.usernameKey, keys), isNull(users.disabledAt)));
  const missing = keys.find((k) => !rows.some((r) => r.key === k));
  if (missing) throw new UnknownUserError(names.find((n) => n.trim().toLowerCase() === missing)!.trim());
  return rows.filter((r) => r.id !== ownerId).map((r) => r.id);
}

/** Replaces the friends tagged in `clipId`. */
export async function setTags(db: Pick<Db, 'delete' | 'insert'>, clipId: string, userIds: string[]): Promise<void> {
  await db.delete(clipTags).where(eq(clipTags.clipId, clipId));
  if (userIds.length > 0) await db.insert(clipTags).values(userIds.map((userId) => ({ clipId, userId })));
}

/** Active users, for picking friends to tag. */
export async function listUsernames(db: Db): Promise<{ username: string }[]> {
  return db.select({ username: users.username }).from(users).where(isNull(users.disabledAt)).orderBy(asc(users.usernameKey));
}

export async function getClip(db: Db, id: string): Promise<ClipWithOwner | null> {
  const [row] = await db
    .select({ clip: clips, owner: { username: users.username }, views: viewsExpr, tagged: taggedExpr })
    .from(clips)
    .innerJoin(users, eq(users.id, clips.userId))
    .where(eq(clips.id, id));
  return row ? { ...row, views: Number(row.views), tagged: row.tagged ?? [] } : null;
}
