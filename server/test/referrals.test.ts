import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import type { FriendList } from '@klipt/shared';
import { invites, users } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
let alice: { id: string; headers: Record<string, string>; cookie: string };

beforeEach(async () => {
  t = await makeTestApp();
  const a = await createUser(t.db, { username: 'alice' });
  const s = await sessionFor(t.db, a.id);
  alice = { id: a.id, headers: s.headers, cookie: s.cookie };
});
afterEach(() => t.close());

const makeLink = async (headers = alice.headers) => {
  const res = await t.app.inject({ method: 'POST', url: '/api/referrals', headers });
  expect(res.statusCode).toBe(200);
  return res.json() as { code: string; url: string; expiresAt: string };
};
const register = (username: string, opts: { ref?: string; inviteCode?: string; password?: string } = {}) =>
  t.app.inject({
    method: 'POST',
    url: '/api/register',
    headers: opts.ref ? { cookie: `klipt_ref=${opts.ref}` } : {},
    payload: { username, password: opts.password ?? 'password123', ...(opts.inviteCode ? { inviteCode: opts.inviteCode } : {}) },
  });
const sessionOf = (res: Awaited<ReturnType<typeof register>>) => `klipt_session=${res.cookies.find((c) => c.name === 'klipt_session')!.value}`;
const friendsOf = async (cookie: string) =>
  ((await t.app.inject({ url: '/api/friends', headers: { cookie } })).json() as FriendList).friends.map((f) => f.username);

describe('open sign-up', () => {
  it('anyone can register without a code', async () => {
    const res = await register('newbie');
    expect(res.statusCode).toBe(200);
    expect(res.json()).toMatchObject({ username: 'newbie', isAdmin: false });
    expect(res.json().referral).toBeUndefined();
    expect(await friendsOf(sessionOf(res))).toEqual([]);
  });

  it('keeps the username and password rules', async () => {
    expect((await register('a b')).json()).toEqual({ error: 'invalid_username' });
    expect((await register('newuser', { password: '1234567' })).json()).toEqual({ error: 'weak_password' });
    expect((await register('ALICE')).statusCode).toBe(409);
  });

  it('accepts an old clip-share sign-up body and ignores the clip', async () => {
    const res = await t.app.inject({ method: 'POST', url: '/api/register', payload: { clipId: 'AAAAAAAAAAAA', username: 'fromshare', password: 'password123' } });
    expect(res.statusCode).toBe(200);
    expect(await friendsOf(sessionOf(res))).toEqual([]);
  });
});

describe('referral links', () => {
  it('register with a referral cookie makes friends and uses the code', async () => {
    const link = await makeLink();
    const res = await register('bob', { ref: link.code });
    expect(res.statusCode).toBe(200);
    expect(res.json().referral).toBeUndefined();
    expect(await friendsOf(sessionOf(res))).toEqual(['alice']);
    expect(await friendsOf(alice.cookie)).toEqual(['bob']);
    // The cookie is cleared.
    expect(res.cookies.find((c) => c.name === 'klipt_ref')?.value).toBe('');

    const again = await register('carol', { ref: link.code });
    expect(again.statusCode).toBe(200);
    expect(again.json().referral).toBe('invalid');
    expect(await friendsOf(sessionOf(again))).toEqual([]);
  });

  it('a code in the body works like the cookie', async () => {
    const link = await makeLink();
    const res = await register('bob', { inviteCode: link.code });
    expect(await friendsOf(sessionOf(res))).toEqual(['alice']);
  });

  it('an expired or revoked referral still signs up, without a friend', async () => {
    const a = await makeLink();
    await t.db.update(invites).set({ expiresAt: new Date(Date.now() - 1000) }).where(eq(invites.code, a.code));
    expect((await register('one', { ref: a.code })).json().referral).toBe('invalid');
    const b = await makeLink();
    expect((await t.app.inject({ method: 'DELETE', url: `/api/referrals/${b.code}`, headers: alice.headers })).statusCode).toBe(204);
    expect((await register('two', { ref: b.code })).json().referral).toBe('invalid');
    expect((await register('three', { ref: 'nonexistent' })).json().referral).toBe('invalid');
  });

  it("a disabled sender's link makes no friend", async () => {
    const link = await makeLink();
    await t.db.update(users).set({ disabledAt: new Date() }).where(eq(users.id, alice.id));
    expect((await register('bob', { ref: link.code })).json().referral).toBe('invalid');
  });

  it('a failed sign-up keeps the code usable', async () => {
    const link = await makeLink();
    expect((await register('ALICE', { ref: link.code })).statusCode).toBe(409);
    expect((await register('bob', { ref: link.code })).json().referral).toBeUndefined();
  });

  it('two sign-ups at once: only one becomes the friend', async () => {
    const link = await makeLink();
    const results = await Promise.all([register('racer1', { ref: link.code }), register('racer2', { ref: link.code })]);
    expect(results.map((r) => r.statusCode)).toEqual([200, 200]);
    expect(results.map((r) => r.json().referral ?? 'ok').sort()).toEqual(['invalid', 'ok']);
    expect(await friendsOf(alice.cookie)).toHaveLength(1);
  });

  it('POST twice gives two codes, 14 days each; GET lists, DELETE revokes', async () => {
    const a = await makeLink();
    const b = await makeLink();
    expect(a.code).not.toBe(b.code);
    expect(a.url).toBe(`http://localhost/r/${a.code}`);
    const days = (new Date(a.expiresAt).getTime() - Date.now()) / 86400000;
    expect(days).toBeGreaterThan(13.99);
    expect(days).toBeLessThanOrEqual(14);
    expect((await t.app.inject({ method: 'DELETE', url: `/api/referrals/${a.code}`, headers: alice.headers })).statusCode).toBe(204);
    const list = (await t.app.inject({ url: '/api/referrals', headers: alice.headers })).json() as { code: string }[];
    expect(list.map((l) => l.code)).toEqual([b.code]);
  });

  it("you can't revoke someone else's link", async () => {
    const link = await makeLink();
    const bob = await createUser(t.db, { username: 'bob' });
    const bobHeaders = (await sessionFor(t.db, bob.id)).headers;
    expect((await t.app.inject({ method: 'DELETE', url: `/api/referrals/${link.code}`, headers: bobHeaders })).statusCode).toBe(404);
  });

  it('needs a login', async () => {
    expect((await t.app.inject({ method: 'POST', url: '/api/referrals' })).statusCode).toBe(401);
  });
});

describe('/r/:code', () => {
  it('logged out: remembers the code and goes to the home page', async () => {
    const link = await makeLink();
    const res = await t.app.inject({ url: `/r/${link.code}` });
    expect(res.statusCode).toBe(302);
    expect(res.headers.location).toBe('/?ref=1');
    const c = res.cookies.find((x) => x.name === 'klipt_ref')!;
    expect(c.value).toBe(link.code);
    expect(c.httpOnly).toBe(true);
    expect(c.maxAge).toBe(14 * 24 * 60 * 60);
  });

  it('logged out, used code: says so', async () => {
    const res = await t.app.inject({ url: '/r/nonexistent' });
    expect(res.headers.location).toBe('/?ref=invalid');
    expect(res.cookies.find((x) => x.name === 'klipt_ref')).toBeUndefined();
  });

  it("logged in: befriends and goes to the sender's profile; the code is used", async () => {
    const link = await makeLink();
    const bob = await createUser(t.db, { username: 'bob' });
    const bobCookie = (await sessionFor(t.db, bob.id)).cookie;
    const res = await t.app.inject({ url: `/r/${link.code}`, headers: { cookie: bobCookie } });
    expect(res.statusCode).toBe(302);
    expect(res.headers.location).toBe('/u/alice');
    expect(await friendsOf(bobCookie)).toEqual(['alice']);
    expect((await register('carol', { ref: link.code })).json().referral).toBe('invalid');
  });

  it('logged in, own or used code', async () => {
    const link = await makeLink();
    expect((await t.app.inject({ url: `/r/${link.code}`, headers: { cookie: alice.cookie } })).headers.location).toBe('/friends?ref=own');
    expect((await t.app.inject({ url: '/r/nonexistent', headers: { cookie: alice.cookie } })).headers.location).toBe('/friends?ref=invalid');
  });

  it('old /invite links go to /r', async () => {
    const res = await t.app.inject({ url: '/invite/abc' });
    expect(res.statusCode).toBe(302);
    expect(res.headers.location).toBe('/r/abc');
  });

  it('admin invite endpoints are gone', async () => {
    const root = await createUser(t.db, { username: 'root', isAdmin: true });
    const headers = (await sessionFor(t.db, root.id)).headers;
    expect((await t.app.inject({ url: '/api/admin/invites', headers })).statusCode).toBe(404);
  });
});

describe('/signup', () => {
  it('is a page with the form; logged in goes to the feed', async () => {
    const res = await t.app.inject({ url: '/signup' });
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('data-form="signup"');
    expect((await t.app.inject({ url: '/signup', headers: { cookie: alice.cookie } })).headers.location).toBe('/');
  });

  it('the login page links to it', async () => {
    expect((await t.app.inject({ url: '/login' })).body).toContain('href="/signup"');
  });
});
