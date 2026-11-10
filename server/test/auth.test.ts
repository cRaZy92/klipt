import { eq } from 'drizzle-orm';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { sessions } from '../src/db/schema.js';
import { createUser, loginAs, makeTestApp, sessionFor, type TestApp } from './helpers.js';

describe('auth', () => {
  let t: TestApp;
  beforeEach(async () => {
    t = await makeTestApp();
    await createUser(t.db, { username: 'adam', password: 'hunter2hunter2' });
  });
  afterEach(async () => {
    await t.close();
  });

  const login = (username: string, password: string, client = 'web') =>
    t.app.inject({ method: 'POST', url: '/api/auth/login', payload: { username, password, client } });

  it('logs in case-insensitively', async () => {
    const res = await login('ADAM', 'hunter2hunter2');
    expect(res.statusCode).toBe(200);
    expect(res.json().username).toBe('adam');
  });

  it('rejects wrong password and unknown user identically', async () => {
    const a = await login('adam', 'nope');
    const b = await login('ghost', 'nope');
    expect(a.statusCode).toBe(401);
    expect(a.json()).toEqual({ error: 'invalid_credentials' });
    expect(b.statusCode).toBe(401);
    expect(b.json()).toEqual({ error: 'invalid_credentials' });
  });

  it('rejects disabled users', async () => {
    await createUser(t.db, { username: 'bob', password: 'password123' });
    const { users } = await import('../src/db/schema.js');
    await t.db.update(users).set({ disabledAt: new Date() }).where(eq(users.usernameKey, 'bob'));
    expect((await login('bob', 'password123')).statusCode).toBe(401);
  });

  it('rate limits login to 10/min per IP', async () => {
    for (let i = 0; i < 10; i++) expect((await login('adam', 'wrong')).statusCode).toBe(401);
    expect((await login('adam', 'wrong')).statusCode).toBe(429);
  });

  it('cannot bypass the login rate limit by forging the leftmost X-Forwarded-For entry', async () => {
    const codes: number[] = [];
    for (let i = 1; i <= 11; i++) {
      const res = await t.app.inject({
        method: 'POST',
        url: '/api/auth/login',
        headers: { 'x-forwarded-for': `1.2.3.${i}, 203.0.113.7` },
        payload: { username: 'adam', password: 'wrong', client: 'web' },
      });
      codes.push(res.statusCode);
    }
    expect(codes.slice(0, 10).every((c) => c === 401)).toBe(true);
    expect(codes[10]).toBe(429);
  });

  it('does not rate limit other routes', async () => {
    for (let i = 0; i < 15; i++) {
      expect((await t.app.inject({ method: 'GET', url: '/api/health' })).statusCode).toBe(200);
    }
  });

  it('web login sets an HttpOnly cookie that authenticates /api/me', async () => {
    const res = await login('adam', 'hunter2hunter2');
    const setCookie = String(res.headers['set-cookie']);
    expect(setCookie).toMatch(/klipt_session=/);
    expect(setCookie).toMatch(/HttpOnly/i);
    expect(setCookie).toMatch(/SameSite=Lax/i);
    expect(setCookie).toMatch(/Path=\//);
    expect(setCookie).not.toMatch(/Secure/i);
    const cookie = `klipt_session=${res.cookies[0]!.value}`;
    const me = await t.app.inject({ method: 'GET', url: '/api/me', headers: { cookie } });
    expect(me.statusCode).toBe(200);
    expect(me.json()).toMatchObject({ username: 'adam', isAdmin: false });
  });

  it('stores only the hash of the token', async () => {
    const token = await loginAs(t.app, 'adam', 'hunter2hunter2', 'desktop');
    const rows = await t.db.select().from(sessions);
    expect(rows).toHaveLength(1);
    expect(rows[0]!.tokenHash).not.toContain(token);
    expect(rows[0]!.expiresAt).toBeNull();
  });

  it('desktop login returns a token usable as Bearer, until logout', async () => {
    const res = await login('adam', 'hunter2hunter2', 'desktop');
    const { token, me } = res.json();
    expect(me.username).toBe('adam');
    const headers = { authorization: `Bearer ${token}` };
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers })).statusCode).toBe(200);
    const out = await t.app.inject({ method: 'POST', url: '/api/auth/logout', headers });
    expect(out.statusCode).toBe(204);
    const after = await t.app.inject({ method: 'GET', url: '/api/me', headers });
    expect(after.statusCode).toBe(401);
    expect(after.json()).toEqual({ error: 'unauthorized' });
  });

  it('logout clears the cookie', async () => {
    const cookie = await loginAs(t.app, 'adam', 'hunter2hunter2');
    const out = await t.app.inject({ method: 'POST', url: '/api/auth/logout', headers: { cookie } });
    expect(out.statusCode).toBe(204);
    expect(String(out.headers['set-cookie'])).toMatch(/klipt_session=;/);
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: { cookie } })).statusCode).toBe(401);
  });

  it('expired web sessions and disabled users resolve to 401', async () => {
    const user = await createUser(t.db, { username: 'carol' });
    const s = await sessionFor(t.db, user.id, 'web');
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: s.headers })).statusCode).toBe(200);
    await t.db.update(sessions).set({ expiresAt: new Date(Date.now() - 1000) }).where(eq(sessions.id, s.session.id));
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: s.headers })).statusCode).toBe(401);

    const d = await sessionFor(t.db, user.id, 'desktop');
    const { users } = await import('../src/db/schema.js');
    await t.db.update(users).set({ disabledAt: new Date() }).where(eq(users.id, user.id));
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: d.headers })).statusCode).toBe(401);
  });

  it('lists own sessions and revokes one; cannot revoke another user\'s', async () => {
    const adam = (await t.db.query.users.findFirst())!;
    const other = await createUser(t.db, { username: 'dave' });
    const mine = await sessionFor(t.db, adam.id, 'web');
    const mine2 = await sessionFor(t.db, adam.id, 'desktop');
    const theirs = await sessionFor(t.db, other.id, 'web');

    const list = await t.app.inject({ method: 'GET', url: '/api/auth/sessions', headers: mine.headers });
    const body = list.json() as { id: string; current: boolean }[];
    expect(body).toHaveLength(2);
    expect(body.find((s) => s.current)!.id).toBe(mine.session.id);

    const bad = await t.app.inject({ method: 'DELETE', url: `/api/auth/sessions/${theirs.session.id}`, headers: mine.headers });
    expect(bad.statusCode).toBe(404);
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: theirs.headers })).statusCode).toBe(200);

    const ok = await t.app.inject({ method: 'DELETE', url: `/api/auth/sessions/${mine2.session.id}`, headers: mine.headers });
    expect(ok.statusCode).toBe(204);
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: mine2.headers })).statusCode).toBe(401);
    const nonUuid = await t.app.inject({ method: 'DELETE', url: '/api/auth/sessions/xyz', headers: mine.headers });
    expect(nonUuid.statusCode).toBe(404);
  });

  it('requires auth for sessions list', async () => {
    expect((await t.app.inject({ method: 'GET', url: '/api/auth/sessions' })).statusCode).toBe(401);
  });
});
