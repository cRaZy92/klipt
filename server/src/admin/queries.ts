import { and, desc, eq, gt, inArray, isNull, sql } from 'drizzle-orm';
import type { AdminStats, AdminTranscodes, AdminUser, Invite, TranscodeJob } from '@klipt/shared';
import { transcodeCounts, type Transcoder } from '../clips/transcoder.js';
import type { Db } from '../db/client.js';
import { clips, invites, users } from '../db/schema.js';

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
      isAdmin: users.isAdmin,
      disabledAt: users.disabledAt,
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
