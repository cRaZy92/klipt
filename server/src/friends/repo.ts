import { and, asc, eq, isNull, or, sql, type SQL } from 'drizzle-orm';
import type { AnyPgColumn } from 'drizzle-orm/pg-core';
import type { FriendState } from '@klipt/shared';
import type { Db } from '../db/client.js';
import { friendships, users } from '../db/schema.js';
import { userRefColumns, type UserRefRow } from '../users/ref.js';

/** A pair as stored: the smaller id first. */
export function pairOf(a: string, b: string): [string, string] {
  return a < b ? [a, b] : [b, a];
}

/** Whether `viewerId` and the user in `ownerCol` are friends (an accepted row between them). */
export function friendsExpr(viewerId: string, ownerCol: AnyPgColumn | SQL): SQL<boolean> {
  return sql<boolean>`exists (select 1 from ${friendships} f where f.status = 'accepted' and (
    (f.user_a = ${viewerId}::uuid and f.user_b = ${ownerCol}) or (f.user_b = ${viewerId}::uuid and f.user_a = ${ownerCol})))`;
}

async function rowOf(db: Pick<Db, 'select'>, a: string, b: string) {
  const [x, y] = pairOf(a, b);
  const [row] = await db.select().from(friendships).where(and(eq(friendships.userA, x), eq(friendships.userB, y)));
  return row ?? null;
}

export async function friendState(db: Db, viewerId: string, otherId: string): Promise<FriendState> {
  if (viewerId === otherId) return 'self';
  const row = await rowOf(db, viewerId, otherId);
  if (!row) return 'none';
  if (row.status === 'accepted') return 'friends';
  return row.requestedBy === viewerId ? 'outgoing' : 'incoming';
}

/** Asks `toId` to be friends, or accepts when they already asked. An existing request or friendship stays. */
export async function requestFriend(db: Db, fromId: string, toId: string): Promise<FriendState> {
  const [userA, userB] = pairOf(fromId, toId);
  await db.insert(friendships).values({ userA, userB, requestedBy: fromId, status: 'pending' }).onConflictDoNothing();
  // Their request to me: asking back accepts it.
  await db
    .update(friendships)
    .set({ status: 'accepted', acceptedAt: new Date() })
    .where(and(eq(friendships.userA, userA), eq(friendships.userB, userB), eq(friendships.status, 'pending'), eq(friendships.requestedBy, toId)));
  return friendState(db, fromId, toId);
}

/** Declines, cancels or unfriends: the row goes, whatever it was. */
export async function removeFriend(db: Db, a: string, b: string): Promise<void> {
  const [userA, userB] = pairOf(a, b);
  await db.delete(friendships).where(and(eq(friendships.userA, userA), eq(friendships.userB, userB)));
}

/** Makes `a` and `b` friends straight away (a referral link). */
export async function makeFriends(db: Pick<Db, 'insert'>, a: string, b: string): Promise<void> {
  const [userA, userB] = pairOf(a, b);
  const now = new Date();
  await db
    .insert(friendships)
    .values({ userA, userB, requestedBy: a, status: 'accepted', acceptedAt: now })
    .onConflictDoUpdate({ target: [friendships.userA, friendships.userB], set: { status: 'accepted', acceptedAt: now } });
}

/** Everyone `userId` has a row with, as `UserRefRow`s with that row's state. Disabled users are left out. */
async function related(db: Db, userId: string): Promise<(UserRefRow & { status: 'pending' | 'accepted'; requestedBy: string })[]> {
  return db
    .select({ ...userRefColumns, status: friendships.status, requestedBy: friendships.requestedBy })
    .from(friendships)
    .innerJoin(users, or(and(eq(friendships.userA, userId), eq(users.id, friendships.userB)), and(eq(friendships.userB, userId), eq(users.id, friendships.userA))))
    .where(isNull(users.disabledAt))
    .orderBy(asc(users.usernameKey));
}

export async function listFriendRows(db: Db, userId: string): Promise<{ friends: UserRefRow[]; incoming: UserRefRow[]; outgoing: UserRefRow[] }> {
  const rows = await related(db, userId);
  const strip = ({ status: _s, requestedBy: _r, ...ref }: (typeof rows)[number]): UserRefRow => ref;
  return {
    friends: rows.filter((r) => r.status === 'accepted').map(strip),
    incoming: rows.filter((r) => r.status === 'pending' && r.requestedBy !== userId).map(strip),
    outgoing: rows.filter((r) => r.status === 'pending' && r.requestedBy === userId).map(strip),
  };
}
