import { and, desc, eq, gt, inArray, isNull, sql } from 'drizzle-orm';
import type { AdminStats, AdminTranscodes, AdminUser, FeedbackItem, Invite, TranscodeJob } from '@klipt/shared';
import { transcodeCounts, type Transcoder } from '../clips/transcoder.js';
import type { Db } from '../db/client.js';
import { clips, feedback, invites, sessions, users } from '../db/schema.js';
import { displayNameOf } from '../clips/repo.js';

/** An invite that is unused, not revoked and not expired. Single source of truth for invite validity. */
export const activeInvite = (now: Date = new Date()) =>
  and(isNull(invites.usedBy), isNull(invites.revokedAt), gt(invites.expiresAt, now))!;

export const inviteUrl = (baseUrl: string, code: string) => `${baseUrl}/invite/${code}`;

export function toInvite(i: { code: string; expiresAt: Date }, baseUrl: string): Invite {
  return { code: i.code, url: inviteUrl(baseUrl, i.code), expiresAt: i.expiresAt.toISOString() };
}

export async function listActiveInvites(db: Db, baseUrl: string): Promise<Invite[]> {
  const rows = await db.select().from(invites).where(activeInvite()).orderBy(invites.createdAt);
  return rows.map((i) => toInvite(i, baseUrl));
}

export async function isInviteActive(db: Db, code: string): Promise<boolean> {
  const [row] = await db
    .select({ code: invites.code })
    .from(invites)
    .where(and(eq(invites.code, code), activeInvite()));
  return !!row;
}

export async function listAdminUsers(db: Db): Promise<AdminUser[]> {
  const rows = await db
    .select({
      id: users.id,
      username: users.username,
      displayName: displayNameOf(users),
      isAdmin: users.isAdmin,
      disabledAt: users.disabledAt,
      lastSeenAt: sql<string | null>`(select max(s.last_used_at) from ${sessions} s where s.user_id = ${users.id})`,
      app: sql<{ version: string | null; os: string | null; gpu: string | null; lastUsedAt: string } | null>`(
        select json_build_object('version', s.app_version, 'os', s.os, 'gpu', s.gpu, 'lastUsedAt', s.last_used_at)
        from ${sessions} s where s.user_id = ${users.id} and s.kind = 'desktop'
        order by (s.app_version is not null) desc, s.last_used_at desc limit 1)`,
      createdAt: users.createdAt,
      clipCount: sql<number>`count(${clips.id})::int`,
      storageBytes: sql<number>`coalesce(sum(${clips.sizeBytes}), 0)::float8`,
    })
    .from(users)
    .leftJoin(clips, eq(clips.userId, users.id))
    .groupBy(users.id)
    .orderBy(users.createdAt);
  return rows.map((u) => ({
    id: u.id,
    username: u.username,
    displayName: u.displayName,
    lastSeenAt: u.lastSeenAt ? new Date(u.lastSeenAt).toISOString() : null,
    app: u.app ? { ...u.app, lastUsedAt: new Date(u.app.lastUsedAt).toISOString() } : null,
    isAdmin: u.isAdmin,
    disabled: u.disabledAt !== null,
    clipCount: u.clipCount,
    storageBytes: Number(u.storageBytes),
    createdAt: u.createdAt.toISOString(),
  }));
}

export async function getAdminStats(db: Db): Promise<AdminStats> {
  const [u] = await db.select({ n: sql<number>`count(*)::int` }).from(users);
  const [c] = await db
    .select({ n: sql<number>`count(*)::int`, bytes: sql<number>`coalesce(sum(${clips.sizeBytes}), 0)::float8` })
    .from(clips);
  return { users: u!.n, clips: c!.n, storageBytes: Number(c!.bytes) };
}

const ACTIVE_LIMIT = 50;
const RECENT_LIMIT = 30;

/** The compression queue for the admin page: what's running, what waits, what finished lately. */
export async function getTranscodes(db: Db, transcoder: Transcoder): Promise<AdminTranscodes> {
  const counts = await transcodeCounts(db);
  const job = {
    clipId: clips.id,
    title: clips.title,
    owner: users.username,
    state: clips.transcodeState,
    durationMs: clips.durationMs,
    sizeBytes: clips.sizeBytes,
    originalBytes: clips.originalBytes,
    error: clips.transcodeError,
    startedAt: clips.transcodeStartedAt,
    finishedAt: clips.transcodeFinishedAt,
  };
  const [active, recent] = await Promise.all([
    db
      .select(job)
      .from(clips)
      .innerJoin(users, eq(users.id, clips.userId))
      .where(inArray(clips.transcodeState, ['running', 'queued']))
      // The running one first, then the order the worker takes them (newest first).
      .orderBy(sql`${clips.transcodeState} = 'running' desc`, desc(clips.createdAt), desc(clips.id))
      .limit(ACTIVE_LIMIT),
    db
      .select(job)
      .from(clips)
      .innerJoin(users, eq(users.id, clips.userId))
      .where(inArray(clips.transcodeState, ['done', 'skipped', 'failed']))
      .orderBy(sql`${clips.transcodeFinishedAt} desc nulls last`, desc(clips.id))
      .limit(RECENT_LIMIT),
  ]);
  const current = transcoder.status();
  const toJob = (r: (typeof active)[number]): TranscodeJob => ({
    ...r,
    state: r.state!,
    startedAt: r.startedAt?.toISOString() ?? null,
    finishedAt: r.finishedAt?.toISOString() ?? null,
    progress: r.state === 'running' ? (current?.clipId === r.clipId ? current.progress : 0) : null,
  });
  const { savedBytes, ...stateCounts } = counts;
  return { enabled: transcoder.enabled, counts: stateCounts, savedBytes, active: active.map(toJob), recent: recent.map(toJob) };
}

export interface WeekActivity {
  /** Monday of the week, YYYY-MM-DD in Slovak time. */
  week: string;
  clips: number;
  bytes: number;
}

const ACTIVITY_WEEKS = 12;
const WEEK_EXPR = sql<string>`to_char(date_trunc('week', ${clips.createdAt} at time zone 'Europe/Bratislava'), 'YYYY-MM-DD')`;

/** Mondays (YYYY-MM-DD, Slovak time) of the last `n` weeks, oldest first, ending with this week. */
export function lastMondays(n: number, now = new Date()): string[] {
  const local = new Intl.DateTimeFormat('en-CA', { timeZone: 'Europe/Bratislava', year: 'numeric', month: '2-digit', day: '2-digit' }).format(now);
  const today = new Date(`${local}T00:00:00Z`);
  const monday = new Date(today.getTime() - ((today.getUTCDay() + 6) % 7) * 86400000);
  return Array.from({ length: n }, (_, i) => new Date(monday.getTime() - (n - 1 - i) * 7 * 86400000).toISOString().slice(0, 10));
}

/** Uploads per week for the admin chart: the last 12 weeks, empty weeks included. */
export async function getActivity(db: Db, now = new Date()): Promise<WeekActivity[]> {
  const weeks = lastMondays(ACTIVITY_WEEKS, now);
  const rows = await db
    .select({ week: WEEK_EXPR, clips: sql<number>`count(*)::int`, bytes: sql<number>`coalesce(sum(${clips.sizeBytes}), 0)::float8` })
    .from(clips)
    // A day of slack on the cutoff; weeks outside the list are dropped below.
    .where(gt(clips.createdAt, new Date(Date.parse(`${weeks[0]}T00:00:00Z`) - 86400000)))
    .groupBy(WEEK_EXPR);
  const byWeek = new Map(rows.map((r) => [r.week, r]));
  return weeks.map((week) => ({ week, clips: Number(byWeek.get(week)?.clips ?? 0), bytes: Number(byWeek.get(week)?.bytes ?? 0) }));
}

export interface GameShare {
  /** null for the "Other games" row that folds in the rest. */
  gameName: string | null;
  clips: number;
}

const TOP_GAMES = 6;

/** Clips per game across the whole server: the top six, then the rest folded into one row. */
export async function getGameShares(db: Db): Promise<GameShare[]> {
  const rows = await db
    .select({ gameName: clips.gameName, clips: sql<number>`count(*)::int` })
    .from(clips)
    .where(sql`${clips.gameName} is not null`)
    .groupBy(clips.gameName)
    .orderBy(sql`count(*) desc`, clips.gameName);
  const top: GameShare[] = rows.slice(0, TOP_GAMES).map((r) => ({ gameName: r.gameName, clips: Number(r.clips) }));
  const rest = rows.slice(TOP_GAMES).reduce((n, r) => n + Number(r.clips), 0);
  return rest > 0 ? [...top, { gameName: null, clips: rest }] : top;
}

/** Problem reports and ideas, newest first (open ones before resolved ones). */
export async function listFeedback(db: Db, limit = 100): Promise<FeedbackItem[]> {
  const rows = await db
    .select({
      id: feedback.id,
      kind: feedback.kind,
      message: feedback.message,
      source: feedback.source,
      system: feedback.system,
      hasLogs: sql<boolean>`(${feedback.logs} is not null and ${feedback.logs} <> '')`,
      createdAt: feedback.createdAt,
      resolvedAt: feedback.resolvedAt,
      username: users.username,
      displayName: displayNameOf(users),
    })
    .from(feedback)
    .innerJoin(users, eq(users.id, feedback.userId))
    .orderBy(sql`${feedback.resolvedAt} is not null`, desc(feedback.createdAt))
    .limit(limit);
  return rows.map((r) => ({
    id: r.id,
    kind: r.kind,
    message: r.message,
    source: r.source,
    system: r.system ?? null,
    hasLogs: !!r.hasLogs,
    createdAt: r.createdAt.toISOString(),
    resolvedAt: r.resolvedAt?.toISOString() ?? null,
    user: { username: r.username, displayName: r.displayName },
  }));
}
