import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { ensureAdmin } from '../src/admin/bootstrap.js';
import { createTestDb } from '../src/db/client.js';
import { clips, sessions, users } from '../src/db/schema.js';
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

  it('forbids non-admins on /api/admin/*', async () => {
    const u = await createUser(t.db, { username: 'plain' });
    const { headers } = await sessionFor(t.db, u.id);
    for (const [method, url] of [
      ['GET', '/api/admin/users'],
      ['GET', '/api/admin/stats'],
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
      storageBytes: number;
      disabled: boolean;
    }[];
    expect(users_.find((x) => x.username === 'filmer')).toMatchObject({ clipCount: 2, storageBytes: 5_000_000_010, disabled: false });
    expect((await t.app.inject({ url: '/api/admin/stats', headers: adminHeaders })).json()).toEqual({
      users: 2,
      clips: 2,
      storageBytes: 5_000_000_010,
      freeBytes: expect.any(Number),
      diskLow: false,
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
