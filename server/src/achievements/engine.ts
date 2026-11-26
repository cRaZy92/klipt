import { and, desc, eq, isNull } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import { achievementById, cosmeticName, earnedIds, sanitizeLooks, unlockedCosmetics, type MyAchievements } from '@klipt/shared';
import type { Db } from '../db/client.js';
import { userAchievements, users } from '../db/schema.js';
import { mergeStats, readActivity, serverStats } from './stats.js';

/**
 * Awards what `userId` has earned and doesn't have yet; returns those ids. `quiet`: earned for
 * past activity (the startup run, the app's first report), announced in one summary.
 */
export async function evaluateUser(db: Db, userId: string, opts: { quiet: boolean }): Promise<string[]> {
  const [user] = await db.select({ activity: users.activity }).from(users).where(eq(users.id, userId));
  if (!user) return [];
  const activity = readActivity(user.activity);
  const ids = earnedIds(mergeStats(await serverStats(db, userId), activity), new Set(activity.firsts));
  if (ids.length === 0) return [];
  const added = await db
    .insert(userAchievements)
    .values(ids.map((achievementId) => ({ userId, achievementId, quiet: opts.quiet })))
    .onConflictDoNothing()
    .returning({ id: userAchievements.achievementId });
  return added.map((r) => r.id);
}

/** The startup run: every active user, quietly (also catches up achievements added since). Returns how many were awarded. */
export async function evaluateAll(db: Db): Promise<number> {
  const all = await db.select({ id: users.id }).from(users).where(isNull(users.disabledAt));
  let n = 0;
  for (const u of all) n += (await evaluateUser(db, u.id, { quiet: true })).length;
  return n;
}

export async function myAchievements(db: Db, userId: string): Promise<MyAchievements> {
  const rows = await db
    .select()
    .from(userAchievements)
    .where(eq(userAchievements.userId, userId))
    .orderBy(desc(userAchievements.earnedAt));
  const [user] = await db.select({ looks: users.looks }).from(users).where(eq(users.id, userId));
  const earned = rows.flatMap((r) => {
    const def = achievementById(r.achievementId);
    if (!def) return [];
    return [
      {
        id: def.id,
        name: def.name,
        description: def.description,
        icon: def.icon,
        earnedAt: r.earnedAt.toISOString(),
        seen: r.seenAt !== null,
        quiet: r.quiet,
        reward: def.reward ?? null,
        rewardName: def.reward ? cosmeticName(def.reward) : null,
      },
    ];
  });
  return { earned, looks: sanitizeLooks(user?.looks), unlocked: [...unlockedCosmetics(rows.map((r) => r.achievementId))] };
}

/** Marks `ids` (all when absent) seen for `userId`. */
export async function markSeen(db: Db, userId: string, ids?: string[]): Promise<void> {
  const where = and(eq(userAchievements.userId, userId), isNull(userAchievements.seenAt));
  const rows = await db.select({ id: userAchievements.achievementId }).from(userAchievements).where(where);
  const pick = ids ? rows.filter((r) => ids.includes(r.id)) : rows;
  for (const r of pick) {
    await db.update(userAchievements).set({ seenAt: new Date() }).where(and(where, eq(userAchievements.achievementId, r.id)));
  }
}

/**
 * Re-checks these users after something changed (an upload, a view, a tag, an album). Never
 * throws: a failure is logged and the request that caused it still succeeds.
 */
export async function touch(app: FastifyInstance, userIds: (string | null | undefined)[]): Promise<void> {
  for (const id of new Set(userIds.filter((u): u is string => !!u))) {
    try {
      await evaluateUser(app.db, id, { quiet: false });
    } catch (err) {
      app.log.error({ err }, 'checking achievements failed');
    }
  }
}
