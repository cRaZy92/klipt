import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { clips, invites } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp, befriendAll } from './helpers.js';
import { safeNext } from '../src/web/routes.js';
import { getActivity, getGameShares, lastMondays } from '../src/admin/queries.js';

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
  await befriendAll(t.db);
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
    // Sessions live on the Account page now.
    expect((await get('/account', a.cookie)).body).toContain('Signed in on');
    expect((await get('/u/nobody', b.cookie)).statusCode).toBe(404);
  });

  it('searches the feed, my clips and user pages, keeping the search in chips', async () => {
    const a = await mk('alice');
    await addClip(a.u.id, { title: 'Clutch ace', gameName: 'CS2' });
    await addClip(a.u.id, { title: 'Team wipe', gameName: 'CS2' });
    for (const path of ['/', '/me', '/u/alice']) {
      const body = (await get(`${path}?q=ace`, a.cookie)).body;
      expect(body, path).toContain('Clutch ace');
      expect(body, path).not.toContain('Team wipe');
      expect(body, path).toMatch(/<input[^>]*type="search"[^>]*name="q"[^>]*value="ace"/);
      expect(body, path).toContain(`?game=CS2&amp;q=ace`);
    }
    const none = (await get('/?q=%3Cnothing%3E', a.cookie)).body;
    expect(none).toContain('No clips match “&lt;nothing&gt;”.');
  });

  it('profile shows clips, views and the most-watched clip, counting only what the viewer may see', async () => {
    const a = await mk('alice');
    const b = await mk('bob');
    const pub = await addClip(a.u.id, { title: 'Popular', gameName: 'CS2', anonViews: 5 });
    await addClip(a.u.id, { title: 'Quiet', anonViews: 1 });
    await addClip(a.u.id, { title: 'Secret', visibility: 'link', anonViews: 50 });
    const theirs = (await get('/u/alice', b.cookie)).body;
    expect(theirs).toMatch(/data-stat="clips"><b>2</);
    expect(theirs).toMatch(/data-stat="views"><b>6</);
    expect(theirs).toMatch(new RegExp(`data-stat="top"><a href="/c/${pub}"><b>Popular</b></a><span>most watched · 5 views<`));
    expect(theirs).not.toContain('Secret');
    const own = (await get('/u/alice', a.cookie)).body;
    expect(own).toMatch(/data-stat="clips"><b>3</);
    expect(own).toMatch(/data-stat="views"><b>56</);
    expect(own).toContain('<b>Secret</b>');
    await mk('carol');
    expect((await get('/u/carol', b.cookie)).body).not.toContain('data-stat=');
  });

  it('feed filters to clips you are in; share page names the friends in it', async () => {
    const a = await mk('alice');
    const b = await mk('bob');
    const withBob = await addClip(a.u.id, { title: 'Duo' });
    await addClip(a.u.id, { title: 'Solo' });
    await t.app.inject({ method: 'PATCH', url: `/api/clips/${withBob}`, headers: { cookie: a.cookie }, payload: { tagged: ['bob'] } });
    const feed = (await get('/?with=me', b.cookie)).body;
    expect(feed).toContain('Duo');
    expect(feed).not.toContain('Solo');
    expect(feed).toMatch(/<a class="chip active" href="\/"[^>]*>I'm in it</);
    expect((await get('/', b.cookie)).body).toMatch(/<a class="chip" href="\/\?with=me"[^>]*>I'm in it</);
    const share = (await get(`/c/${withBob}`, b.cookie)).body;
    expect(share).toMatch(/data-role="tagged">With <a href="\/u\/bob">bob<\/a></);
    expect((await get(`/c/${withBob}`, a.cookie)).body).toContain('data-action="tag"');
    expect(share).not.toContain('data-action="tag"');
    expect((await get('/u/bob', a.cookie)).body).toContain('Duo');
  });

  it('me page shows how much server storage your clips use', async () => {
    const a = await mk('alice');
    const b = await mk('bob');
    await addClip(a.u.id, { sizeBytes: 1_000_000_000 });
    await addClip(a.u.id, { sizeBytes: 610_000_000, visibility: 'link' });
    await addClip(b.u.id, { sizeBytes: 5_000_000_000 });
    expect((await get('/me', a.cookie)).body).toMatch(/data-role="storage">2 clips, 1\.5 GB on the server</);
    const c = await mk('carol');
    expect((await get('/me', c.cookie)).body).not.toContain('data-role="storage"');
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
    expect(res.body).toContain('Uploads per week');
  });

  it('admin charts: uploads per week in Slovak time, and the top games with the rest folded', async () => {
    const a = await mk('alice');
    const now = new Date('2026-11-18T12:00:00Z'); // a Wednesday
    // Sunday 23:30 UTC is already Monday in Bratislava: it counts for this week.
    await addClip(a.u.id, { createdAt: new Date('2026-11-15T23:30:00Z'), sizeBytes: 100, gameName: 'G1' });
    await addClip(a.u.id, { createdAt: new Date('2026-11-15T21:00:00Z'), sizeBytes: 50, gameName: 'G1' });
    await addClip(a.u.id, { createdAt: new Date('2026-08-01T12:00:00Z') }); // older than 12 weeks
    const weeks = await getActivity(t.db, now);
    expect(weeks).toHaveLength(12);
    expect(weeks[11]).toEqual({ week: '2026-11-16', clips: 1, bytes: 100 });
    expect(weeks[10]).toEqual({ week: '2026-11-09', clips: 1, bytes: 50 });
    expect(weeks.slice(0, 10).every((w) => w.clips === 0)).toBe(true);

    for (const [game, n] of [['G2', 3], ['G3', 2], ['G4', 2], ['G5', 2], ['G6', 1], ['G7', 1], ['G8', 1]] as const) {
      for (let i = 0; i < n; i++) await addClip(a.u.id, { gameName: game });
    }
    const games = await getGameShares(t.db);
    expect(games.map((g) => g.gameName)).toEqual(['G2', 'G1', 'G3', 'G4', 'G5', 'G6', null]);
    expect(games.at(-1)).toEqual({ gameName: null, clips: 2 });
  });

  it('lastMondays counts back from the Monday of this week in Slovak time', () => {
    expect(lastMondays(3, new Date('2026-11-15T23:30:00Z'))).toEqual(['2026-11-02', '2026-11-09', '2026-11-16']);
    expect(lastMondays(1, new Date('2026-11-15T22:30:00Z'))).toEqual(['2026-11-09']);
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
    expect(ok.body).toContain('data-next="/get-app?welcome=1"');
    await t.db.update(invites).set({ revokedAt: new Date() }).where(eq(invites.code, 'goodcode'));
    expect((await get('/invite/goodcode')).body).toContain('no longer valid');
  });

  it('serves the install guide publicly and links it from the top bar', async () => {
    const pub = await get('/get-app');
    expect(pub.statusCode).toBe(200);
    expect(pub.body).toContain('Get the Klipt app');
    expect(pub.body).toContain('href="/get-app/windows"');
    expect(pub.body).toContain('Run anyway');
    expect(pub.body).not.toContain('Go to the feed');

    const a = await mk('alice');
    expect((await get('/', a.cookie)).body).toContain('<a href="/get-app">Get the app</a>');
    const welcome = await get('/get-app?welcome=1&next=%2Fc%2Fabc', a.cookie);
    expect(welcome.body).toContain('Welcome, alice!');
    expect(welcome.body).toContain('<strong>alice</strong>');
    expect(welcome.body).toContain('href="/c/abc">Back to the clip');
    const evil = await get('/get-app?next=%2F%2Fevil.com', a.cookie);
    expect(evil.body).toContain('href="/">Go to the feed');
    expect(evil.body).not.toContain('Welcome');
  });

  it('renders HTML 404 for unknown pages and JSON for api', async () => {
    const res = await get('/nothing-here');
    expect(res.statusCode).toBe(404);
    expect(res.headers['content-type']).toContain('text/html');
    expect((await get('/api/nothing')).headers['content-type']).toContain('json');
  });
});

describe('formatSize', () => {
  it('uses MB below a gigabyte and GB with one decimal above', async () => {
    const { formatSize } = await import('../src/web/components.js');
    expect(formatSize(0)).toBe('0 MB');
    expect(formatSize(10)).toBe('0.1 MB');
    expect(formatSize(340 * 1024 ** 2)).toBe('340 MB');
    expect(formatSize(1.5 * 1024 ** 3)).toBe('1.5 GB');
    expect(formatSize(12 * 1024 ** 3)).toBe('12 GB');
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
