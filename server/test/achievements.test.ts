import { mkdtemp, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { eq, sql } from 'drizzle-orm';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { EarnedAchievement, MyAchievements } from '@klipt/shared';
import { clips, sessions, userAchievements, users } from '../src/db/schema.js';
import { evaluateAll } from '../src/achievements/engine.js';
import { createUser, makeFixtureVideo, makeTestApp, sessionFor, tusUpload, type TestApp, befriendAll } from './helpers.js';

let t: TestApp;
let a: { id: string; h: Record<string, string>; token: string };
let b: { id: string; h: Record<string, string>; token: string };
let seq = 0;

async function mk(name: string) {
  const u = await createUser(t.db, { username: name });
  await befriendAll(t.db);
  const s = await sessionFor(t.db, u.id, 'desktop');
  return { id: u.id, h: s.headers, token: s.token };
}

async function addClip(userId: string, extra: Partial<typeof clips.$inferInsert> = {}) {
  seq++;
  const id = `clip${String(seq).padStart(8, '0')}`;
  await t.db.insert(clips).values({ id, userId, title: `Clip ${seq}`, durationMs: 10_000, width: 1, height: 1, sizeBytes: 1, ...extra });
  return id;
}

const mine = async (h: Record<string, string>): Promise<MyAchievements> =>
  (await t.app.inject({ method: 'GET', url: '/api/me/achievements', headers: h })).json();
const find = (m: MyAchievements, id: string): EarnedAchievement | undefined => m.earned.find((e) => e.id === id);
const report = (h: Record<string, string>, payload: unknown) =>
  t.app.inject({ method: 'POST', url: '/api/me/activity', headers: h, payload: payload as object });

beforeEach(async () => {
  seq = 0;
  t = await makeTestApp();
  a = await mk('alice');
  b = await mk('bob');
});
afterEach(() => t.close());

describe('achievements', () => {
  it('an upload earns loud achievements that are new until seen', async () => {
    const fixture = await readFile(await makeFixtureVideo(await mkdtemp(join(tmpdir(), 'klipt-ach-'))));
    const { uploadId } = await tusUpload(t.app, a.token, fixture);
    const res = await t.app.inject({ method: 'POST', url: '/api/clips', headers: a.h, payload: { uploadId, title: 'x' } });
    expect(res.statusCode).toBe(201);
    const m = await mine(a.h);
    expect(find(m, 'first-upload')).toMatchObject({ quiet: false, seen: false, name: 'Going live' });
    expect(find(m, 'first-clip')).toBeDefined();
    // A 2-second fixture is a blink.
    expect(find(m, 'blink')).toBeDefined();
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: a.h })).json().newAchievements).toBeGreaterThanOrEqual(2);
    expect((await t.app.inject({ method: 'POST', url: '/api/me/achievements/seen', headers: a.h, payload: {} })).statusCode).toBe(204);
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: a.h })).json().newAchievements).toBe(0);
    expect(find(await mine(a.h), 'first-upload')!.seen).toBe(true);
  });

  it('marks only the given ids seen', async () => {
    await report(a.h, { counters: { screenshots: 1, recordings: 1 }, firsts: [] });
    await t.app.inject({ method: 'POST', url: '/api/me/achievements/seen', headers: a.h, payload: { ids: ['first-screenshot'] } });
    const m = await mine(a.h);
    expect(find(m, 'first-screenshot')!.seen).toBe(true);
    expect(find(m, 'first-recording')!.seen).toBe(false);
  });

  it('the startup run is quiet and does nothing twice', async () => {
    for (let i = 0; i < 10; i++) await addClip(a.id);
    expect(await evaluateAll(t.db)).toBeGreaterThan(0);
    const first = find(await mine(a.h), 'clips-10')!;
    expect(first).toMatchObject({ quiet: true, seen: false });
    expect(await evaluateAll(t.db)).toBe(0);
    const again = find(await mine(a.h), 'clips-10')!;
    expect(again.earnedAt).toBe(first.earnedAt);
    expect(again.quiet).toBe(true);
    expect((await t.db.select().from(userAchievements).where(eq(userAchievements.userId, a.id))).filter((r) => r.achievementId === 'clips-10')).toHaveLength(1);
  });

  it('keeps badges when the clips are gone', async () => {
    for (let i = 0; i < 10; i++) await addClip(a.id);
    await evaluateAll(t.db);
    await t.db.delete(clips);
    await evaluateAll(t.db);
    expect(find(await mine(a.h), 'clips-10')).toBeDefined();
  });

  it('keeps the highest counters and every first across reports', async () => {
    const res = await report(a.h, { counters: { recordedMs: 40_000_000 }, firsts: ['cut'] });
    expect(res.statusCode).toBe(200);
    const m = res.json() as MyAchievements;
    expect(find(m, 'hours-10')).toMatchObject({ quiet: false, reward: 'theme:ember', rewardName: 'Ember' });
    expect(find(m, 'first-cut')).toBeDefined();
    expect(m.unlocked).toContain('theme:ember');
    await report(a.h, { counters: { recordedMs: 5 }, firsts: ['zoom'] });
    const [row] = await t.db.select({ activity: users.activity }).from(users).where(eq(users.id, a.id));
    expect(row!.activity).toEqual({ counters: { recordedMs: 40_000_000 }, firsts: ['cut', 'zoom'] });
  });

  it('a backfill report awards quietly', async () => {
    const m = (await report(a.h, { counters: { clipsSaved: 3 }, firsts: ['nightOwl'], backfill: true })).json() as MyAchievements;
    expect(find(m, 'first-clip')!.quiet).toBe(true);
    expect(find(m, 'night-owl')!.quiet).toBe(true);
    expect(m.unlocked).toContain('theme:midnight');
  });

  it('validates reports but lets unknown keys through unstored', async () => {
    expect((await report(a.h, { counters: { clipsSaved: -1 }, firsts: [] })).statusCode).toBe(400);
    expect((await report(a.h, { counters: { clipsSaved: 1.5 }, firsts: [] })).statusCode).toBe(400);
    expect((await report(a.h, { counters: { future: 3 }, firsts: ['someday'] })).statusCode).toBe(200);
    const [row] = await t.db.select({ activity: users.activity }).from(users).where(eq(users.id, a.id));
    expect(row!.activity).toEqual({ counters: {}, firsts: [] });
    expect((await report({}, { counters: {}, firsts: [] })).statusCode).toBe(401);
  });

  it('counts views, anonymous ones too, and seen by everyone among active users', async () => {
    const carol = await createUser(t.db, { username: 'carol' });
    await t.db.update(users).set({ disabledAt: new Date() }).where(eq(users.id, carol.id));
    const id = await addClip(a.id);
    await t.db.update(clips).set({ anonViews: 9 }).where(eq(clips.id, id));
    await t.app.inject({ method: 'POST', url: `/api/clips/${id}/view`, headers: b.h });
    const m = await mine(a.h);
    expect(find(m, 'views-10')).toBeDefined();
    expect(find(m, 'seen-by-everyone')).toMatchObject({ reward: 'frame:rainbow' });
  });

  it('a long-idle user does not block seen by everyone', async () => {
    const dave = await mk('dave');
    await t.db.update(sessions).set({ lastUsedAt: new Date(Date.now() - 40 * 86_400_000) }).where(eq(sessions.userId, dave.id));
    const id = await addClip(a.id);
    await t.app.inject({ method: 'POST', url: `/api/clips/${id}/view`, headers: b.h });
    expect(find(await mine(a.h), 'seen-by-everyone')).toBeDefined();
  });

  it('earns editor-all from firsts over two reports', async () => {
    await report(a.h, { counters: {}, firsts: ['cut', 'zoom', 'subtitle', 'blur'] });
    expect(find(await mine(a.h), 'editor-all')).toBeUndefined();
    await report(a.h, { counters: {}, firsts: ['mutePart', 'combine', 'part', 'thumbnailImage'] });
    expect(find(await mine(a.h), 'editor-all')).toMatchObject({ reward: 'banner:editor' });
  });

  it('tags count for both sides', async () => {
    const id = await addClip(a.id);
    await t.app.inject({ method: 'PATCH', url: `/api/clips/${id}`, headers: a.h, payload: { tagged: ['bob'] } });
    expect(find(await mine(a.h), 'first-tag')).toBeDefined();
    for (let i = 0; i < 9; i++) {
      const other = await addClip(a.id);
      await t.app.inject({ method: 'PATCH', url: `/api/clips/${other}`, headers: a.h, payload: { tagged: ['bob'] } });
    }
    expect(find(await mine(b.h), 'tagged-10')).toBeDefined();
  });

  it('an album of 10 clips makes a curator', async () => {
    const ids = [];
    for (let i = 0; i < 10; i++) ids.push(await addClip(b.id));
    const album = (await t.app.inject({ method: 'POST', url: '/api/albums', headers: a.h, payload: { title: 'Best', clipIds: ids.slice(0, 9) } })).json();
    expect(find(await mine(a.h), 'curator')).toBeUndefined();
    await t.app.inject({ method: 'PUT', url: `/api/albums/${album.id}/clips/${ids[9]}`, headers: a.h });
    expect(find(await mine(a.h), 'curator')).toBeDefined();
  });

  it('games count from the server and the app, whichever is higher', async () => {
    for (const g of ['A', 'B', 'C', 'D']) await addClip(a.id, { gameName: g });
    await report(a.h, { counters: { games: 2 }, firsts: [] });
    expect(find(await mine(a.h), 'games-5')).toBeUndefined();
    await report(a.h, { counters: { games: 5 }, firsts: [] });
    expect(find(await mine(a.h), 'games-5')).toBeDefined();
  });

  it('old clips, new year uploads and archivists', async () => {
    await addClip(a.id, { createdAt: new Date(Date.now() - 400 * 86_400_000) });
    await addClip(b.id);
    await t.db.execute(sql`update clips set created_at = make_timestamptz(2026, 1, 1, 12, 0, 0) where user_id = ${b.id}`);
    await evaluateAll(t.db);
    expect(find(await mine(a.h), 'archivist')).toBeDefined();
    expect(find(await mine(b.h), 'new-year')).toBeDefined();
    expect(find(await mine(b.h), 'archivist')).toBeUndefined();
  });
});
