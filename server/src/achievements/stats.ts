import { eq, sql } from 'drizzle-orm';
import {
  ACTIVITY_COUNTERS,
  ACTIVITY_FIRSTS,
  type ActivityCounter,
  type ActivityFirst,
  type ActivityReport,
  type ServerStat,
  type StatKey,
} from '@klipt/shared';
import type { Db } from '../db/client.js';
import { users } from '../db/schema.js';

/** What the desktop app reported for a user, as `users.activity` keeps it. */
export interface StoredActivity {
  counters: Partial<Record<ActivityCounter, number>>;
  firsts: ActivityFirst[];
}

export type ServerStats = Record<ServerStat, number> & { distinctGames: number };

/** "Active" for Seen by everyone: not disabled and used Klipt in the last 30 days. */
const activeOther = sql`o.id <> ${users.id} and o.disabled_at is null
  and exists (select 1 from sessions s where s.user_id = o.id and s.last_used_at > now() - interval '30 days')`;

/** Everything the server counts for `userId` (over what it holds now; earned badges stay regardless). */
export async function serverStats(db: Db, userId: string): Promise<ServerStats> {
  const n = (q: ReturnType<typeof sql>) => sql<number>`(${q})::int`;
  const [row] = await db
    .select({
      uploads: n(sql`select count(*) from clips c where c.user_id = ${users.id} and c.kind = 'clip'`),
      anyUploads: n(sql`select count(*) from clips c where c.user_id = ${users.id}`),
      totalViews: n(sql`select coalesce(sum(c.anon_views + (select count(*) from clip_views v where v.clip_id = c.id)), 0) from clips c where c.user_id = ${users.id}`),
      taggedIn: n(sql`select count(*) from clip_tags t join clips c on c.id = t.clip_id where t.user_id = ${users.id} and c.user_id <> ${users.id}`),
      tagsGiven: n(sql`select count(*) from clip_tags t join clips c on c.id = t.clip_id where c.user_id = ${users.id}`),
      maxAlbumClips: n(sql`select coalesce(max((select count(*) from album_clips ac where ac.album_id = a.id)), 0) from albums a where a.user_id = ${users.id}`),
      seenByEveryone: n(sql`select case when not exists (select 1 from users o where ${activeOther}) then 0 else (
        select count(*) from clips c where c.user_id = ${users.id} and not exists (
          select 1 from users o where ${activeOther}
            and not exists (select 1 from clip_views v where v.clip_id = c.id and v.user_id = o.id))) end`),
      blinkUploads: n(sql`select count(*) from clips c where c.user_id = ${users.id} and c.kind = 'clip' and c.duration_ms < 5000`),
      newYearUploads: n(sql`select count(*) from clips c where c.user_id = ${users.id} and extract(month from c.created_at) = 1 and extract(day from c.created_at) = 1`),
      yearOld: n(sql`select count(*) from clips c where c.user_id = ${users.id} and c.created_at <= now() - interval '365 days'`),
      distinctGames: n(sql`select count(distinct c.game_name) from clips c where c.user_id = ${users.id} and c.game_name is not null`),
    })
    .from(users)
    .where(eq(users.id, userId));
  const out = {} as ServerStats;
  for (const [k, v] of Object.entries(row ?? {})) (out as Record<string, number>)[k] = Number(v);
  return out;
}

/** The stats the rules read: the server's, plus the app's counters (clips saved and games: whichever is higher). */
export function mergeStats(server: ServerStats, activity: StoredActivity): Partial<Record<StatKey, number>> {
  const { distinctGames, ...rest } = server;
  return {
    ...activity.counters,
    ...rest,
    clipsSaved: Math.max(activity.counters.clipsSaved ?? 0, server.anyUploads),
    games: Math.max(activity.counters.games ?? 0, distinctGames),
  };
}

/** Stored activity as it can be used (unknown keys and bad values dropped). */
export function readActivity(raw: unknown): StoredActivity {
  const r = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  const counters: StoredActivity['counters'] = {};
  const c = r.counters && typeof r.counters === 'object' ? (r.counters as Record<string, unknown>) : {};
  for (const key of ACTIVITY_COUNTERS) {
    const v = c[key];
    if (typeof v === 'number' && Number.isFinite(v) && v >= 0) counters[key] = v;
  }
  const firsts = Array.isArray(r.firsts) ? ACTIVITY_FIRSTS.filter((f) => (r.firsts as unknown[]).includes(f)) : [];
  return { counters, firsts };
}

/** A report folded into what's stored: counters keep the highest value seen (two PCs don't fight), firsts are a union. */
export function mergeActivity(stored: StoredActivity | null, report: ActivityReport): StoredActivity {
  const base = stored ?? { counters: {}, firsts: [] };
  const counters = { ...base.counters };
  for (const key of ACTIVITY_COUNTERS) {
    const v = report.counters[key];
    if (v !== undefined) counters[key] = Math.max(counters[key] ?? 0, v);
  }
  const firsts = new Set<string>([...base.firsts, ...report.firsts]);
  return { counters, firsts: ACTIVITY_FIRSTS.filter((f) => firsts.has(f)) };
}
