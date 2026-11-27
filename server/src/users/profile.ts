import { and, eq, isNull, sql } from 'drizzle-orm';
import { BANNERS, type EarnedAchievement, type FriendState, type UserProfile } from '@klipt/shared';
import type { AuthUser } from '../auth/sessions.js';
import type { Db } from '../db/client.js';
import { users } from '../db/schema.js';
import { myAchievements } from '../achievements/engine.js';
import { getProfileStats, type ProfileStats } from '../clips/repo.js';
import { friendState } from '../friends/repo.js';
import { toUserRef, userRefColumns, type UserRefRow } from './ref.js';

export interface LoadedProfile {
  owner: UserRefRow;
  /** False: a Friends profile seen by someone who isn't a friend (or logged out). */
  full: boolean;
  banner: string | null;
  stats: ProfileStats | null;
  badges: EarnedAchievement[];
  friendState: FriendState | null;
  joinedAt: Date;
}

/**
 * A profile's head as `viewer` may see it; null for an unknown or disabled user. Anyone sees a
 * Public profile in full; a Friends profile shows strangers only the name and picture.
 */
export async function loadProfile(db: Db, viewer: AuthUser | null, username: string): Promise<LoadedProfile | null> {
  if (username.length > 64) return null;
  const [owner] = await db
    .select({ ...userRefColumns, banner: sql<string | null>`${users.looks}->>'banner'`, profileVisibility: users.profileVisibility, joinedAt: users.createdAt })
    .from(users)
    .where(and(eq(users.usernameKey, username.toLowerCase()), isNull(users.disabledAt)));
  if (!owner) return null;
  const state = viewer ? await friendState(db, viewer.id, owner.id) : null;
  const full = owner.profileVisibility === 'public' || state === 'self' || state === 'friends';
  const [stats, achievements] = await Promise.all([
    full ? getProfileStats(db, viewer, owner.username) : null,
    full ? myAchievements(db, owner.id) : null,
  ]);
  const { banner, profileVisibility: _, joinedAt, ...ref } = owner;
  return {
    owner: ref,
    full,
    banner: full && BANNERS.some((b) => b.id === banner) ? banner : null,
    stats,
    badges: achievements?.earned ?? [],
    friendState: state,
    joinedAt,
  };
}

export function toUserProfile(p: LoadedProfile, baseUrl: string): UserProfile {
  return {
    user: toUserRef(p.owner, baseUrl),
    full: p.full,
    banner: p.banner,
    stats: p.stats,
    badges: p.badges.map(({ id, name, description, icon }) => ({ id, name, description, icon })),
    friendState: p.friendState,
    profileUrl: `${baseUrl}/u/${encodeURIComponent(p.owner.username)}`,
    joinedAt: p.joinedAt.toISOString(),
  };
}
