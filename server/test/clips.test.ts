import { existsSync } from 'node:fs';
import { copyFile, mkdir, mkdtemp, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { sql } from 'drizzle-orm';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { clips } from '../src/db/schema.js';
import { createUser, makeFixtureVideo, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
let a: { id: string; h: Record<string, string> };
let b: { id: string; h: Record<string, string> };
let admin: { id: string; h: Record<string, string> };
let seq = 0;

async function mk(name: string, opts: { isAdmin?: boolean } = {}) {
  const u = await createUser(t.db, { username: name, ...opts });
  return { id: u.id, h: (await sessionFor(t.db, u.id)).headers };
}

async function addClip(
  userId: string,
  extra: Partial<typeof clips.$inferInsert> = {},
  files = true,
) {
  seq++;
  const id = `clip${String(seq).padStart(8, '0')}`;
  await t.db.insert(clips).values({
    id,
    userId,
    title: `Clip ${seq}`,
    durationMs: 1000,
    width: 320,
    height: 240,
    sizeBytes: 10,
    createdAt: new Date(Date.UTC(2026, 0, 1, 0, 0, seq)),
    ...extra,
  });
  if (files) {
    const dir = join(t.dataDir, 'clips');
    await mkdir(dir, { recursive: true });
    await writeFile(join(dir, `${id}.mp4`), 'x');
    await writeFile(join(dir, `${id}.jpg`), 'x');
  }
  return id;
}

const get = (url: string, h: Record<string, string>) => t.app.inject({ method: 'GET', url, headers: h });

beforeEach(async () => {
  seq = 0;
  t = await makeTestApp();
  a = await mk('alice');
  b = await mk('bob');
  admin = await mk('root', { isAdmin: true });
});
afterEach(() => t.close());

describe('clip listing and visibility', () => {
  it('hides link clips from other users but not by direct id', async () => {
    await addClip(a.id, { gameName: 'Doom', visibility: 'feed' });
    const linkId = await addClip(a.id, { gameName: 'Quake', visibility: 'link' });
    expect((await get('/api/clips?scope=feed', b.h)).json().items).toHaveLength(1);
    expect((await get('/api/clips?scope=user&username=alice', b.h)).json().items).toHaveLength(1);
    expect((await get('/api/clips?scope=user&username=ALICE', a.h)).json().items).toHaveLength(2);
    expect((await get('/api/clips?scope=mine', a.h)).json().items).toHaveLength(2);
    expect((await get('/api/clips?scope=mine', b.h)).json().items).toHaveLength(0);
    expect((await get('/api/games?scope=feed', b.h)).json()).toEqual([{ gameName: 'Doom', steamAppId: null, count: 1 }]);
    const direct = await get(`/api/clips/${linkId}`, b.h);
    expect(direct.statusCode).toBe(200);
    expect(direct.json().owner.username).toBe('alice');
  });

  it('requires auth and returns empty for unknown users', async () => {
    expect((await t.app.inject({ method: 'GET', url: '/api/clips' })).statusCode).toBe(401);
    expect((await get('/api/clips?scope=user&username=nobody', a.h)).json()).toEqual({ items: [], nextCursor: null });
    expect((await get('/api/clips/doesnotexist', a.h)).statusCode).toBe(404);
  });

  it('pages without overlap', async () => {
    for (let i = 0; i < 30; i++) await addClip(a.id, {}, false);
    const p1 = (await get('/api/clips?scope=feed', b.h)).json();
    expect(p1.items).toHaveLength(24);
    expect(p1.nextCursor).toEqual(expect.any(String));
    const p2 = (await get(`/api/clips?scope=feed&cursor=${p1.nextCursor}`, b.h)).json();
    expect(p2.items).toHaveLength(6);
    expect(p2.nextCursor).toBeNull();
    const ids = [...p1.items, ...p2.items].map((c: { id: string }) => c.id);
    expect(new Set(ids).size).toBe(30);
  });

  it('pages stably across identical timestamps', async () => {
    const same = new Date(Date.UTC(2026, 5, 1));
    for (let i = 0; i < 5; i++) await addClip(a.id, { createdAt: same }, false);
    const p1 = (await get('/api/clips?scope=feed&limit=3', b.h)).json();
    const p2 = (await get(`/api/clips?scope=feed&limit=3&cursor=${p1.nextCursor}`, b.h)).json();
    expect([...p1.items, ...p2.items].map((c: { id: string }) => c.id)).toHaveLength(5);
    expect(new Set([...p1.items, ...p2.items].map((c: { id: string }) => c.id)).size).toBe(5);
  });

  it('rejects malformed cursors with 400', async () => {
    for (const c of ['!!!', 'bm9waXBl', Buffer.from('2026-01-01T00:00:00.000Z').toString('base64url'), Buffer.from('garbage|x').toString('base64url')]) {
      const res = await get(`/api/clips?scope=feed&cursor=${c}`, a.h);
      expect(res.statusCode, c).toBe(400);
      expect(res.json()).toEqual({ error: 'invalid_cursor' });
    }
  });

  it('filters by game exactly and counts games by popularity', async () => {
    await addClip(a.id, { gameName: 'Overwatch 2', steamAppId: 2357570 });
    await addClip(a.id, { gameName: 'Overwatch 2', steamAppId: 2357570 });
    await addClip(a.id, { gameName: 'Overwatch' });
    await addClip(a.id, {});
    const res = (await get(`/api/clips?scope=feed&game=${encodeURIComponent('Overwatch 2')}`, b.h)).json();
    expect(res.items).toHaveLength(2);
    expect((await get('/api/games?scope=feed', b.h)).json()).toEqual([
      { gameName: 'Overwatch 2', steamAppId: 2357570, count: 2 },
      { gameName: 'Overwatch', steamAppId: null, count: 1 },
    ]);
  });
});

describe('update and delete', () => {
  it('only the owner can patch', async () => {
    const id = await addClip(a.id);
    expect((await t.app.inject({ method: 'PATCH', url: `/api/clips/${id}`, headers: b.h, payload: { title: 'x' } })).statusCode).toBe(403);
    expect((await t.app.inject({ method: 'PATCH', url: `/api/clips/${id}`, headers: admin.h, payload: { title: 'x' } })).statusCode).toBe(403);
    const ok = await t.app.inject({ method: 'PATCH', url: `/api/clips/${id}`, headers: a.h, payload: { title: 'New', visibility: 'link' } });
    expect(ok.statusCode).toBe(200);
    const got = (await get(`/api/clips/${id}`, a.h)).json();
    expect(got).toMatchObject({ title: 'New', visibility: 'link' });
    const bad = await t.app.inject({ method: 'PATCH', url: `/api/clips/${id}`, headers: a.h, payload: { title: '' } });
    expect(bad.statusCode).toBe(400);
  });

  it('owner or admin can delete; others get 403; files are removed', async () => {
    const mine = await addClip(a.id);
    const theirs = await addClip(b.id);
    expect((await t.app.inject({ method: 'DELETE', url: `/api/clips/${mine}`, headers: b.h })).statusCode).toBe(403);
    const del = await t.app.inject({ method: 'DELETE', url: `/api/clips/${theirs}`, headers: admin.h });
    expect(del.statusCode).toBe(204);
    expect(existsSync(join(t.dataDir, 'clips', `${theirs}.mp4`))).toBe(false);
    expect(existsSync(join(t.dataDir, 'clips', `${theirs}.jpg`))).toBe(false);
    expect((await get(`/api/clips/${theirs}`, a.h)).statusCode).toBe(404);
    expect((await t.app.inject({ method: 'DELETE', url: `/api/clips/${mine}`, headers: a.h })).statusCode).toBe(204);
  });
});

describe('search', () => {
  it('matches every word against the title, game or uploader, ignoring case', async () => {
    const ace = await addClip(a.id, { title: 'Clutch ACE on Mirage', gameName: 'Counter-Strike 2' }, false);
    const wipe = await addClip(b.id, { title: 'Team wipe', gameName: 'Active Matter' }, false);
    await addClip(b.id, { title: 'Boring', gameName: null }, false);
    await addClip(b.id, { title: 'ace but hidden', visibility: 'link' }, false);
    const ids = async (q: string) =>
      ((await t.app.inject({ url: `/api/clips?q=${encodeURIComponent(q)}`, headers: a.h })).json() as { items: { id: string }[] }).items.map((c) => c.id);
    expect(await ids('ace')).toEqual([ace]);
    expect(await ids('counter mirage')).toEqual([ace]);
    expect(await ids('matter')).toEqual([wipe]);
    expect(await ids('BOB wipe')).toEqual([wipe]);
    expect(await ids('ace wipe')).toEqual([]);
    expect(await ids('100%')).toEqual([]);
    expect(await ids('  ')).toHaveLength(3);
  });
});

describe('views', () => {
  const view = (id: string, h?: Record<string, string>) =>
    t.app.inject({ method: 'POST', url: `/api/clips/${id}/view`, headers: h ?? {} });
  const viewsOf = async (id: string) => ((await t.app.inject({ url: `/api/clips/${id}`, headers: b.h })).json() as { views: number }).views;

  it('counts each friend once, every logged-out play, and never the owner', async () => {
    const id = await addClip(a.id, {}, false);
    expect(await viewsOf(id)).toBe(0);
    expect((await view(id, a.h)).statusCode).toBe(204); // the owner: not counted
    expect((await view(id, b.h)).statusCode).toBe(204);
    await view(id, b.h);
    await view(id, admin.h);
    await view(id);
    await view(id);
    expect(await viewsOf(id)).toBe(4);
    const list = (await t.app.inject({ url: '/api/clips?scope=mine', headers: a.h })).json() as { items: { id: string; views: number }[] };
    expect(list.items.find((c) => c.id === id)?.views).toBe(4);
  });

  it('404s for unknown clips', async () => {
    expect((await view('doesnotexist', b.h)).statusCode).toBe(404);
    expect((await view('bad!', b.h)).statusCode).toBe(404);
  });

  it('tells only the owner which friends watched, first viewer first', async () => {
    const id = await addClip(a.id, {}, false);
    await view(id, admin.h);
    await view(id, b.h);
    await view(id);
    const res = await t.app.inject({ url: `/api/clips/${id}/viewers`, headers: a.h });
    expect(res.statusCode).toBe(200);
    expect((res.json() as { username: string }[]).map((v) => v.username)).toEqual(['root', 'bob']);
    expect((await t.app.inject({ url: `/api/clips/${id}/viewers`, headers: b.h })).statusCode).toBe(403);
    expect((await t.app.inject({ url: `/api/clips/${id}/viewers` })).statusCode).toBe(401);
  });
});

describe('tagging friends', () => {
  const patch = (id: string, body: object, h = a.h) =>
    t.app.inject({ method: 'PATCH', url: `/api/clips/${id}`, headers: h, payload: body });
  const list = async (url: string, h = b.h) =>
    ((await t.app.inject({ url, headers: h })).json() as { items: { id: string; tagged: string[] }[] }).items.map((c) => c.id);

  it('the owner tags friends by name; unknown names, the owner and duplicates are refused or dropped', async () => {
    const id = await addClip(a.id, {}, false);
    const res = await patch(id, { tagged: ['Bob', 'root', 'bob', 'alice'] });
    expect(res.statusCode).toBe(200);
    expect((res.json() as { tagged: string[] }).tagged).toEqual(['bob', 'root']);
    expect((await patch(id, { tagged: ['nobody'] })).json()).toEqual({ error: 'unknown_user', message: 'No user called nobody.' });
    expect((await patch(id, { tagged: ['bob'] }, b.h)).statusCode).toBe(403);
    expect(((await patch(id, { tagged: [] })).json() as { tagged: string[] }).tagged).toEqual([]);
  });

  it('a tagged feed clip shows on the friend\'s profile and under "with me"', async () => {
    const tagged = await addClip(a.id, {}, false);
    const hidden = await addClip(a.id, { visibility: 'link' }, false);
    const other = await addClip(a.id, {}, false);
    await patch(tagged, { tagged: ['bob'] });
    await patch(hidden, { tagged: ['bob'] });
    expect(await list('/api/clips?scope=user&username=bob', admin.h)).toEqual([tagged]);
    expect(await list('/api/clips?scope=feed&with=me')).toEqual([tagged]);
    expect(await list('/api/clips?scope=feed&with=me', admin.h)).toEqual([]);
    expect(await list('/api/clips?scope=user&username=alice', admin.h)).toEqual([other, tagged]);
  });

  it('lists active users to tag', async () => {
    const res = await t.app.inject({ url: '/api/users', headers: a.h });
    expect(res.statusCode).toBe(200);
    expect((res.json() as { username: string }[]).map((u) => u.username)).toEqual(['alice', 'bob', 'root']);
    expect((await t.app.inject({ url: '/api/users' })).statusCode).toBe(401);
  });
});

describe('my storage', () => {
  it('counts only your own clips, private ones included', async () => {
    await addClip(a.id, { sizeBytes: 3_000_000_000 }, false);
    await addClip(a.id, { sizeBytes: 500, visibility: 'link' }, false);
    await addClip(b.id, { sizeBytes: 7 }, false);
    const res = await t.app.inject({ url: '/api/me/storage', headers: a.h });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toEqual({ clips: 2, bytes: 3_000_000_500 });
    expect((await t.app.inject({ url: '/api/me/storage', headers: admin.h })).json()).toEqual({ clips: 0, bytes: 0 });
    expect((await t.app.inject({ url: '/api/me/storage' })).statusCode).toBe(401);
  });
});

describe('media serving', () => {
  it('serves ranges publicly with immutable caching', async () => {
    const id = await addClip(a.id, {}, false);
    const dir = await mkdtemp(join(tmpdir(), 'klipt-fx-'));
    const fx = await makeFixtureVideo(dir, { seconds: 3 });
    expect((await stat(fx)).size).toBeGreaterThan(100);
    await mkdir(join(t.dataDir, 'clips'), { recursive: true });
    await copyFile(fx, join(t.dataDir, 'clips', `${id}.mp4`));
    const res = await t.app.inject({ method: 'GET', url: `/media/${id}.mp4`, headers: { range: 'bytes=0-99' } });
    expect(res.statusCode).toBe(206);
    expect(res.headers['content-length']).toBe('100');
    expect(res.headers['accept-ranges']).toBe('bytes');
    expect(res.headers['cache-control']).toBe('public, max-age=31536000, immutable');
    const full = await t.app.inject({ method: 'GET', url: `/media/${id}.mp4` });
    expect(full.statusCode).toBe(200);
  });

  it('sends the video as a download named after the clip title on ?download=1', async () => {
    const id = await addClip(a.id, { title: 'Clutch 1v3 / ő "ace"' }, false);
    await mkdir(join(t.dataDir, 'clips'), { recursive: true });
    await writeFile(join(t.dataDir, 'clips', `${id}.mp4`), Buffer.alloc(64));
    const res = await t.app.inject({ method: 'GET', url: `/media/${id}.mp4?download=1` });
    expect(res.statusCode).toBe(200);
    expect(res.headers['content-disposition']).toBe(
      `attachment; filename="Clutch 1v3 _ _ _ace_.mp4"; filename*=UTF-8''Clutch%201v3%20_%20%C5%91%20_ace_.mp4`,
    );
    const plain = await t.app.inject({ method: 'GET', url: `/media/${id}.mp4` });
    expect(plain.headers['content-disposition']).toBeUndefined();
  });

  it('404s for unknown, malformed, traversal and listing requests', async () => {
    const id = await addClip(a.id, {}, false);
    for (const u of ['/media/nope.mp4', `/media/${id}.mp4`, '/media/', '/media', '/media/..%2F..%2Fetc%2Fpasswd', '/media/%2e%2e%2fconfig.mp4', '/media/.abcdefghijk.mp4', `/media/${id}.mp4.tmp`]) {
      const res = await t.app.inject({ method: 'GET', url: u });
      expect(res.statusCode, u).toBe(404);
      expect(res.headers['cache-control'] ?? '').not.toContain('immutable');
    }
  });
});

describe('cursor precision', () => {
  it('pages with limit=1 through clips created within the same millisecond', async () => {
    for (const us of ['123900', '123500', '123100']) {
      await addClip(a.id, {}, false);
      await t.db.execute(sql`update clips set created_at = ${`2026-03-01 00:00:00.${us}+00`}::timestamptz where id = ${`clip${String(seq).padStart(8, '0')}`}`);
    }
    await t.db.execute(sql`insert into clips (id, user_id, title, duration_ms, width, height, size_bytes) values ('rawnow000001', ${a.id}, 'n1', 1, 1, 1, 1), ('rawnow000002', ${a.id}, 'n2', 1, 1, 1, 1)`);
    const seen: string[] = [];
    let cursor: string | null = null;
    do {
      const res: { items: { id: string }[]; nextCursor: string | null } = (await get(`/api/clips?scope=feed&limit=1${cursor ? `&cursor=${cursor}` : ''}`, b.h)).json();
      seen.push(...res.items.map((c: { id: string }) => c.id));
      cursor = res.nextCursor;
    } while (cursor && seen.length < 20);
    expect(seen).toHaveLength(5);
    expect(new Set(seen).size).toBe(5);
  });
});
