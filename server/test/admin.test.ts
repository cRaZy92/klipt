import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { ensureAdmin } from '../src/admin/bootstrap.js';
import { createTestDb } from '../src/db/client.js';
import { clips, invites, sessions, users } from '../src/db/schema.js';
import { createUser, loginAs, makeTestApp, sessionFor, type TestApp } from './helpers.js';

describe('ensureAdmin', () => {
  it('creates once, then exists; throws without credentials', async () => {
    const db = await createTestDb();
    await expect(ensureAdmin(db, {})).rejects.toThrow('ADMIN_USERNAME and ADMIN_PASSWORD must be set on first start');
    expect(await ensureAdmin(db, { adminUsername: 'Boss', adminPassword: 'password123' })).toBe('created');
    expect(await ensureAdmin(db, {})).toBe('exists');
    const [u] = await db.select().from(users);
    expect(u!.isAdmin).toBe(true);
    expect(u!.usernameKey).toBe('boss');
  });
});

describe('admin + registration', () => {
  let t: TestApp;
  let admin: Awaited<ReturnType<typeof createUser>>;
  let adminHeaders: Record<string, string>;
  beforeEach(async () => {
    t = await makeTestApp();
    admin = await createUser(t.db, { username: 'root', isAdmin: true });
    adminHeaders = (await sessionFor(t.db, admin.id)).headers;
  });
  afterEach(() => t.close());

  const mkInvite = async () => {
    const res = await t.app.inject({ method: 'POST', url: '/api/admin/invites', headers: adminHeaders });
    expect(res.statusCode).toBe(200);
    return res.json() as { code: string; url: string; expiresAt: string };
  };
  const register = (inviteCode: string, username: string, password = 'password123') =>
    t.app.inject({ method: 'POST', url: '/api/register', payload: { inviteCode, username, password } });

  it('creates, lists and revokes invites', async () => {
    const inv = await mkInvite();
    expect(inv.code).toHaveLength(16);
    expect(inv.url).toBe(`http://localhost/invite/${inv.code}`);
    const days = (new Date(inv.expiresAt).getTime() - Date.now()) / 86400000;
    expect(days).toBeGreaterThan(6.9);
    expect(days).toBeLessThanOrEqual(7);
    const list = await t.app.inject({ url: '/api/admin/invites', headers: adminHeaders });
    expect((list.json() as { code: string }[]).map((i) => i.code)).toEqual([inv.code]);
    const del = await t.app.inject({ method: 'DELETE', url: `/api/admin/invites/${inv.code}`, headers: adminHeaders });
    expect(del.statusCode).toBe(204);
    const list2 = await t.app.inject({ url: '/api/admin/invites', headers: adminHeaders });
    expect(list2.json()).toEqual([]);
  });

  it('registers with a valid invite and logs in; invite is single-use', async () => {
    const inv = await mkInvite();
    const res = await register(inv.code, 'Adam');
    expect(res.statusCode).toBe(200);
    const cookie = res.cookies.find((c) => c.name === 'klipt_session');
    expect(cookie?.httpOnly).toBe(true);
    const me = await t.app.inject({ url: '/api/me', headers: { cookie: `klipt_session=${cookie!.value}` } });
    expect(me.json()).toMatchObject({ username: 'Adam', isAdmin: false });
    const again = await register(inv.code, 'Other');
    expect(again.statusCode).toBe(400);
    expect(again.json()).toEqual({ error: 'invalid_invite' });
    const list = await t.app.inject({ url: '/api/admin/invites', headers: adminHeaders });
    expect(list.json()).toEqual([]);
  });

  it('rejects expired and revoked invites', async () => {
    const a = await mkInvite();
    await t.db.update(invites).set({ expiresAt: new Date(Date.now() - 1000) }).where(eq(invites.code, a.code));
    expect((await register(a.code, 'userone')).json()).toEqual({ error: 'invalid_invite' });
    const b = await mkInvite();
    await t.app.inject({ method: 'DELETE', url: `/api/admin/invites/${b.code}`, headers: adminHeaders });
    expect((await register(b.code, 'usertwo')).statusCode).toBe(400);
    expect((await register('nonexistent', 'userthree')).statusCode).toBe(400);
  });

  it('validates username/password and uniqueness without consuming the invite', async () => {
    await createUser(t.db, { username: 'adam' });
    const inv = await mkInvite();
    const taken = await register(inv.code, 'Adam');
    expect(taken.statusCode).toBe(409);
    expect(taken.json()).toEqual({ error: 'username_taken' });
    const bad = await register(inv.code, 'a b');
    expect(bad.statusCode).toBe(400);
    expect(bad.json()).toEqual({ error: 'invalid_username' });
    const weak = await register(inv.code, 'newuser', '1234567');
    expect(weak.statusCode).toBe(400);
    expect(weak.json()).toEqual({ error: 'weak_password' });
    // invite still usable after the failures
    expect((await register(inv.code, 'newuser')).statusCode).toBe(200);
  });

  it('concurrent registrations cannot both use one invite', async () => {
    const inv = await mkInvite();
    const results = await Promise.all([register(inv.code, 'racer1'), register(inv.code, 'racer2')]);
    expect(results.map((r) => r.statusCode).sort()).toEqual([200, 400]);
  });

  it('forbids non-admins on /api/admin/*', async () => {
    const u = await createUser(t.db, { username: 'plain' });
    const { headers } = await sessionFor(t.db, u.id);
    for (const [method, url] of [
      ['GET', '/api/admin/users'],
      ['GET', '/api/admin/stats'],
      ['GET', '/api/admin/invites'],
      ['POST', '/api/admin/invites'],
    ] as const) {
      expect((await t.app.inject({ method, url, headers })).statusCode).toBe(403);
    }
    expect((await t.app.inject({ url: '/api/admin/users' })).statusCode).toBe(401);
  });

  it('lists users and stats', async () => {
    const u = await createUser(t.db, { username: 'filmer' });
    await t.db.insert(clips).values([
      { id: 'clip00000001', userId: u.id, title: 'a', durationMs: 1, width: 1, height: 1, sizeBytes: 5_000_000_000 },
      { id: 'clip00000002', userId: u.id, title: 'b', durationMs: 1, width: 1, height: 1, sizeBytes: 10 },
    ]);
    const ur = await t.app.inject({ url: '/api/admin/users', headers: adminHeaders });
    const users_ = ur.json() as {
      username: string;
      clipCount: number;
      disabled: boolean;
    }[];
    expect(users_.find((x) => x.username === 'filmer')).toMatchObject({ clipCount: 2, disabled: false });
    expect((await t.app.inject({ url: '/api/admin/stats', headers: adminHeaders })).json()).toEqual({
      users: 2,
      clips: 2,
      storageBytes: 5_000_000_010,
    });
  });

  it('cannot disable self', async () => {
    const res = await t.app.inject({
      method: 'PATCH',
      url: `/api/admin/users/${admin.id}`,
      headers: adminHeaders,
      payload: { disabled: true },
    });
    expect(res.statusCode).toBe(400);
  });

  it('disabling a user revokes web and desktop sessions and blocks login', async () => {
    const u = await createUser(t.db, { username: 'victim', password: 'password123' });
    const web = await sessionFor(t.db, u.id, 'web');
    const desk = await sessionFor(t.db, u.id, 'desktop');
    expect((await t.app.inject({ url: '/api/me', headers: web.headers })).statusCode).toBe(200);
    expect((await t.app.inject({ url: '/api/me', headers: desk.headers })).statusCode).toBe(200);
    const res = await t.app.inject({
      method: 'PATCH',
      url: `/api/admin/users/${u.id}`,
      headers: adminHeaders,
      payload: { disabled: true },
    });
    expect(res.statusCode).toBe(204);
    expect((await t.app.inject({ url: '/api/me', headers: web.headers })).statusCode).toBe(401);
    expect((await t.app.inject({ url: '/api/me', headers: desk.headers })).statusCode).toBe(401);
    await expect(loginAs(t.app, 'victim', 'password123')).rejects.toThrow('401');
    expect(await t.db.select().from(sessions).where(eq(sessions.userId, u.id))).toHaveLength(0);
    const re = await t.app.inject({
      method: 'PATCH',
      url: `/api/admin/users/${u.id}`,
      headers: adminHeaders,
      payload: { disabled: false },
    });
    expect(re.statusCode).toBe(204);
    expect((await t.app.inject({ url: '/api/me', headers: web.headers })).statusCode).toBe(401);
    expect((await t.app.inject({ url: '/api/me', headers: desk.headers })).statusCode).toBe(401);
  });
});
