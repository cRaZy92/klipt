import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { clips, invites } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';
import { safeNext } from '../src/web/routes.js';

let t: TestApp;
let seq = 0;

async function addClip(userId: string, extra: Partial<typeof clips.$inferInsert> = {}) {
  seq++;
  const id = `webclip${String(seq).padStart(5, '0')}`;
  await t.db.insert(clips).values({
    id,
    userId,
    title: `Clip ${seq}`,
    durationMs: 65_000,
    width: 320,
    height: 240,
    sizeBytes: 10,
    createdAt: new Date(Date.UTC(2026, 0, 1, 0, 0, seq)),
    ...extra,
  });
  return id;
}

const get = (url: string, cookie?: string) =>
  t.app.inject({ method: 'GET', url, headers: cookie ? { cookie } : {} });

beforeEach(async () => {
  t = await makeTestApp();
});
afterEach(async () => {
  await t.close();
});

async function mk(name: string, isAdmin = false) {
  const u = await createUser(t.db, { username: name, isAdmin });
  return { u, cookie: (await sessionFor(t.db, u.id)).cookie };
}

describe('web pages', () => {
  it('redirects logged-out visitors to login with next', async () => {
    const res = await get('/');
    expect(res.statusCode).toBe(302);
    expect(res.headers.location).toBe('/login?next=%2F');
    expect((await get('/me')).headers.location).toBe('/login?next=%2Fme');
  });

  it('serves login publicly and static assets', async () => {
    const res = await get('/login');
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('<form');
    expect((await get('/static/app.css')).statusCode).toBe(200);
    expect((await get('/static/app.js')).statusCode).toBe(200);
  });

  it('keeps a safe next path and drops open-redirect targets', async () => {
    const ok = await get('/login?next=%2Fu%2Falice%3Fgame%3DCS2%26x%3D1');
    expect(ok.body).toContain('data-next="/u/alice?game=CS2&amp;x=1"');
    for (const bad of ['//evil.com', 'https://evil.com', '/\\evil.com']) {
      const res = await get(`/login?next=${encodeURIComponent(bad)}`);
      expect(res.body).toContain('data-next="/"');
    }
  });

  it('drops non-ASCII and non-printable next paths instead of failing the redirect', async () => {
    expect(safeNext('/u/alice?game=CS2&x=1')).toBe('/u/alice?game=CS2&x=1');
    for (const bad of ['/€', '/caf\u00e9', '/a\x7fb', '/a\u2028b']) expect(safeNext(bad), JSON.stringify(bad)).toBe('/');
    const a = await mk('alice');
    const res = await get('/login?next=%2F%E2%82%AC', a.cookie);
    expect(res.statusCode).toBe(302);
    expect(res.headers.location).toBe('/');
  });

  it('escapes & in attributes so user strings round-trip', async () => {
    const a = await mk('alice');
    await addClip(a.u.id, { title: 'R&amp;D', gameName: 'A&amp;B', steamAppId: 730 });
    const body = (await get('/me', a.cookie)).body;
    expect(body).toContain('data-title="R&amp;amp;D"');
    expect(body).toContain('title="A&amp;amp;B"');
    expect(body).toContain('<span class="chip-name">A&amp;amp;B</span>');
    const adm = await mk('boss', true);
    await t.db.insert(invites).values({ code: 'x&amp;y', createdBy: adm.u.id, expiresAt: new Date(Date.now() + 1e6) });
    expect((await get(`/invite/${encodeURIComponent('x&amp;y')}`)).body).toContain('value="x&amp;amp;y"');
  });

  it('shows feed clips but not link-only clips', async () => {
    const a = await mk('alice');
    const b = await mk('bob');
    await addClip(a.u.id, { title: 'Feed Ace' });
    await addClip(a.u.id, { title: 'Hidden Link', visibility: 'link' });
    const res = await get('/', b.cookie);
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('Feed Ace');
    expect(res.body).toContain('1:05');
    expect(res.body).not.toContain('Hidden Link');
  });

  it('filters by game and renders game chips', async () => {
    const a = await mk('alice');
    await addClip(a.u.id, { title: 'Val One', gameName: 'Valorant' });
    await addClip(a.u.id, { title: 'Cs One', gameName: 'CS2', steamAppId: 730 });
    const res = await get('/?game=Valorant', a.cookie);
    expect(res.body).toContain('Val One');
    expect(res.body).not.toContain('Cs One');
    expect(res.body).toContain('src="/steam-art/730"');
  });

  it('escapes user strings', async () => {
    const a = await mk('alice');
    await addClip(a.u.id, { title: '<script>alert(1)</script>' });
    const res = await get('/', a.cookie);
    expect(res.body).toContain('&lt;script&gt;');
    expect(res.body).not.toContain('<script>alert(1)');
  });

  it('escapes quotes in attributes', async () => {
    const a = await mk('alice');
    await addClip(a.u.id, { title: 'x" onmouseover="alert(1)', gameName: 'G"><b>' });
    const body = (await get('/me', a.cookie)).body;
    expect(body).not.toContain('" onmouseover="');
    expect(body).toContain('G&quot;&gt;&lt;b&gt;');
  });

  it('user page shows only feed clips of others; me shows own link clips', async () => {
    const a = await mk('alice');
    const b = await mk('bob');
    await addClip(a.u.id, { title: 'Pub' });
    await addClip(a.u.id, { title: 'Priv', visibility: 'link' });
    const u = await get('/u/alice', b.cookie);
    expect(u.body).toContain('Pub');
    expect(u.body).not.toContain('Priv');
    const me = await get('/me', a.cookie);
    expect(me.body).toContain('Priv');
    expect(me.body).toContain('Sessions');
    expect((await get('/u/nobody', b.cookie)).statusCode).toBe(404);
  });

  it('me page says where each clip is visible and the button says what it does', async () => {
    const a = await mk('alice');
    await addClip(a.u.id, { title: 'Pub' });
    await addClip(a.u.id, { title: 'Priv', visibility: 'link' });
    const body = (await get('/me', a.cookie)).body;
    expect(body).toContain('Hide from feed');
    expect(body).toContain('Show in feed');
    expect(body).toMatch(/data-role="visibility">In feed</);
    expect(body).toMatch(/data-role="visibility">Link only</);
  });

  it('admin page is admin only', async () => {
    const a = await mk('alice');
    const adm = await mk('boss', true);
    expect((await get('/admin', a.cookie)).statusCode).toBe(404);
    const res = await get('/admin', adm.cookie);
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('Create invite');
    expect(res.body).toContain('GB');
    expect(res.body).toContain('No open invites');
  });

  it('invite page validates code', async () => {
    const adm = await mk('boss', true);
    const a = await mk('alice');
    await t.db.insert(invites).values({ code: 'usedcode', createdBy: adm.u.id, expiresAt: new Date(Date.now() + 1e6), usedBy: a.u.id });
    await t.db.insert(invites).values({ code: 'goodcode', createdBy: adm.u.id, expiresAt: new Date(Date.now() + 1e6) });
    expect((await get('/invite/usedcode')).body).toContain('no longer valid');
    expect((await get('/invite/nope')).body).toContain('no longer valid');
    const ok = await get('/invite/goodcode');
    expect(ok.body).not.toContain('no longer valid');
    expect(ok.body).toContain('goodcode');
    await t.db.update(invites).set({ revokedAt: new Date() }).where(eq(invites.code, 'goodcode'));
    expect((await get('/invite/goodcode')).body).toContain('no longer valid');
  });

  it('renders HTML 404 for unknown pages and JSON for api', async () => {
    const res = await get('/nothing-here');
    expect(res.statusCode).toBe(404);
    expect(res.headers['content-type']).toContain('text/html');
    expect((await get('/api/nothing')).headers['content-type']).toContain('json');
  });
});

describe('skDate', () => {
  it('formats dates the Slovak way, in Slovak time', async () => {
    const { skDate } = await import('../src/web/components.js');
    expect(skDate(new Date(Date.UTC(2026, 10, 10, 12)))).toBe('10. 11. 2026');
    // 23:30 UTC on 10 Nov is already 11 Nov in Bratislava (UTC+1).
    expect(skDate('2026-11-10T23:30:00Z')).toBe('11. 11. 2026');
    expect(skDate(new Date(Date.UTC(2026, 0, 5, 12)))).toBe('5. 1. 2026');
  });
});
