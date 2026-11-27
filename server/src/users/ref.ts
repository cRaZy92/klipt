import { and, eq, isNull, sql } from 'drizzle-orm';
import { isFrame, sanitizeLooks, type Me, type ProfileVisibility, type UserRef } from '@klipt/shared';
import type { Db } from '../db/client.js';
import { userAchievements, users } from '../db/schema.js';

/** The name a user shows to friends: their display name, or their username without one. */
export const displayNameOf = (t: { displayName: unknown; username: unknown }) =>
  sql<string>`coalesce(nullif(${t.displayName}, ''), ${t.username})`;

/** Select these (joined on `users`) to build a `UserRef` with `toUserRef`. */
export const userRefColumns = {
  id: users.id,
  username: users.username,
  displayName: displayNameOf(users),
  avatarVersion: users.avatarVersion,
  frame: sql<string | null>`${users.looks}->>'frame'`,
};
export interface UserRefRow {
  id: string;
  username: string;
  displayName: string;
  avatarVersion: number;
  frame: string | null;
}

/** The picture's URL (`baseUrl` '' gives a path on this site); null without one (version 0 or below). */
export function avatarUrl(u: { id: string; avatarVersion: number }, baseUrl: string): string | null {
  return u.avatarVersion > 0 ? `${baseUrl}/avatars/${u.id}.jpg?v=${u.avatarVersion}` : null;
}

export function toUserRef(r: UserRefRow, baseUrl: string): UserRef {
  return {
    username: r.username,
    displayName: r.displayName,
    avatarUrl: avatarUrl(r, baseUrl),
    frame: isFrame(r.frame) ? r.frame : null,
  };
}

/** How many achievements `userId` hasn't seen yet. */
export const unseenCountExpr = (userId: unknown) =>
  sql<number>`(select count(*) from ${userAchievements} ua where ua.user_id = ${userId} and ua.seen_at is null)::int`;

export function toMe(
  u: { id: string; username: string; displayName: string | null; isAdmin: boolean; avatarVersion: number; looks: unknown; profileVisibility: ProfileVisibility },
  baseUrl: string,
  newAchievements: number,
): Me {
  return {
    id: u.id,
    username: u.username,
    displayName: u.displayName || u.username,
    customName: u.displayName || null,
    isAdmin: u.isAdmin,
    avatarUrl: avatarUrl(u, baseUrl),
    looks: sanitizeLooks(u.looks),
    newAchievements,
    profileVisibility: u.profileVisibility,
  };
}

/** A user as `GET /api/me` answers; null when gone or disabled. */
export async function meOf(db: Db, userId: string, baseUrl: string): Promise<Me | null> {
  const [row] = await db
    .select({ user: users, unseen: unseenCountExpr(users.id) })
    .from(users)
    .where(and(eq(users.id, userId), isNull(users.disabledAt)));
  return row ? toMe(row.user, baseUrl, Number(row.unseen)) : null;
}
