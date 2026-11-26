import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { cleanDisplayName } from '../src/auth/routes.js';
import { clips, sessions } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

describe('cleanDisplayName', () => {
  it('trims, collapses spaces, clears blanks and refuses long or control text', () => {
    expect(cleanDisplayName('  Big   Al ')).toBe('Big Al');
    expect(cleanDisplayName('   ')).toBeNull();
    expect(cleanDisplayName(null)).toBeNull();
    expect(cleanDisplayName('x'.repeat(33))).toBe(false);
    expect(cleanDisplayName('a\u0000b')).toBe(false);
    expect(cleanDisplayName('Šimon 🎮')).toBe('Šimon 🎮');
  });
});

describe('account', () => {
  let t: TestApp;
  let alice: Awaited<ReturnType<typeof createUser>>;
  let headers: Record<string, string>;
  beforeEach(async () => {
    t = await makeTestApp();
    alice = await createUser(t.db, { username: 'alice' });
    headers = (await sessionFor(t.db, alice.id)).headers;
  });
  afterEach(() => t.close());

  it('sets and clears a display name that shows on clips and in /api/me', async () => {
    let res = await t.app.inject({ method: 'PATCH', url: '/api/me', headers, payload: { displayName: ' Ali ' } });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toMatchObject({ username: 'alice', displayName: 'Ali', customName: 'Ali' });

    await t.db.insert(clips).values({ id: 'abcdefghijkl', userId: alice.id, title: 'x', durationMs: 1000, width: 1, height: 1, sizeBytes: 1 });
    const list = await t.app.inject({ method: 'GET', url: '/api/clips?scope=feed', headers });
    expect(list.json().items[0].owner).toEqual({ username: 'alice', displayName: 'Ali', avatarUrl: null, frame: null });
    // Searching finds the display name too.
    const found = await t.app.inject({ method: 'GET', url: '/api/clips?scope=feed&q=ali', headers });
    expect(found.json().items).toHaveLength(1);
    const page = await t.app.inject({ method: 'GET', url: '/c/abcdefghijkl', headers });
    expect(page.body).toContain('Ali');

    res = await t.app.inject({ method: 'PATCH', url: '/api/me', headers, payload: { displayName: '' } });
    expect(res.json()).toMatchObject({ displayName: 'alice', customName: null });
    res = await t.app.inject({ method: 'PATCH', url: '/api/me', headers, payload: { displayName: 'y'.repeat(40) } });
    expect(res.statusCode).toBe(400);
    expect(res.json().error).toBe('invalid_display_name');
  });

  it('changes the password, signing out the other sessions only', async () => {
    const other = await sessionFor(t.db, alice.id, 'desktop');
    let res = await t.app.inject({ method: 'POST', url: '/api/me/password', headers, payload: { currentPassword: 'nope1234', newPassword: 'newpassword1' } });
    expect(res.json().error).toBe('wrong_password');
    res = await t.app.inject({ method: 'POST', url: '/api/me/password', headers, payload: { currentPassword: 'password123', newPassword: 'short' } });
    expect(res.json().error).toBe('weak_password');
    res = await t.app.inject({ method: 'POST', url: '/api/me/password', headers, payload: { currentPassword: 'password123', newPassword: 'newpassword1' } });
    expect(res.statusCode).toBe(204);
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers })).statusCode).toBe(200);
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: other.headers })).statusCode).toBe(401);
    const login = await t.app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'alice', password: 'newpassword1', client: 'web' } });
    expect(login.statusCode).toBe(200);
  });

  it('keeps what the desktop app reports and shows it to admins', async () => {
    const desktop = await sessionFor(t.db, alice.id, 'desktop');
    const res = await t.app.inject({
      method: 'POST',
      url: '/api/me/device',
      headers: desktop.headers,
      payload: { appVersion: '0.1.60', os: 'Windows 11 (26100)', gpu: 'AMD Radeon RX 7800 XT' },
    });
    expect(res.statusCode).toBe(204);
    const [row] = await t.db.select().from(sessions).where(eq(sessions.id, desktop.session.id));
    expect(row).toMatchObject({ appVersion: '0.1.60', os: 'Windows 11 (26100)', gpu: 'AMD Radeon RX 7800 XT' });

    const admin = await createUser(t.db, { username: 'root', isAdmin: true });
    const adminHeaders = (await sessionFor(t.db, admin.id)).headers;
    const users = (await t.app.inject({ method: 'GET', url: '/api/admin/users', headers: adminHeaders })).json();
    const a = users.find((u: { username: string }) => u.username === 'alice');
    expect(a.app).toMatchObject({ version: '0.1.60', os: 'Windows 11 (26100)', gpu: 'AMD Radeon RX 7800 XT' });
    expect(a.lastSeenAt).toBeTruthy();
    const page = await t.app.inject({ method: 'GET', url: '/admin', headers: adminHeaders });
    expect(page.body).toContain('AMD Radeon RX 7800 XT');
  });

  it('lets an admin make a one-time password link', async () => {
    const admin = await createUser(t.db, { username: 'root', isAdmin: true });
    const adminHeaders = (await sessionFor(t.db, admin.id)).headers;
    expect((await t.app.inject({ method: 'POST', url: `/api/admin/users/${alice.id}/password-reset`, headers })).statusCode).toBe(403);
    const first = (await t.app.inject({ method: 'POST', url: `/api/admin/users/${alice.id}/password-reset`, headers: adminHeaders })).json();
    const made = await t.app.inject({ method: 'POST', url: `/api/admin/users/${alice.id}/password-reset`, headers: adminHeaders });
    expect(made.statusCode).toBe(200);
    const code = (made.json().url as string).split('/reset/')[1]!;
    const firstCode = (first.url as string).split('/reset/')[1]!;

    // A newer link replaces the older one.
    expect((await t.app.inject({ method: 'GET', url: `/reset/${firstCode}` })).body).toContain('Link not valid');
    expect((await t.app.inject({ method: 'GET', url: `/reset/${code}` })).body).toContain('alice');

    let res = await t.app.inject({ method: 'POST', url: '/api/password-reset', payload: { code, newPassword: 'short' } });
    expect(res.json().error).toBe('weak_password');
    res = await t.app.inject({ method: 'POST', url: '/api/password-reset', payload: { code, newPassword: 'brandnew123' } });
    expect(res.statusCode).toBe(200);
    expect(res.cookies.some((c) => c.name === 'klipt_session')).toBe(true);
    // Old sessions are gone; the link works once.
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers })).statusCode).toBe(401);
    res = await t.app.inject({ method: 'POST', url: '/api/password-reset', payload: { code, newPassword: 'another123' } });
    expect(res.json().error).toBe('invalid_reset');
    const login = await t.app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'alice', password: 'brandnew123', client: 'web' } });
    expect(login.statusCode).toBe(200);
  });

  it('takes problem reports with logs and lets admins read and resolve them', async () => {
    const res = await t.app.inject({
      method: 'POST',
      url: '/api/feedback',
      headers,
      payload: { kind: 'bug', message: 'Hotkey does nothing in CS2', source: 'desktop', system: { GPU: 'RTX 4070' }, logs: 'line 1\nline 2' },
    });
    expect(res.statusCode).toBe(201);
    expect((await t.app.inject({ method: 'POST', url: '/api/feedback', headers, payload: { kind: 'idea', message: '  ', source: 'web' } })).statusCode).toBe(400);
    expect((await t.app.inject({ method: 'POST', url: '/api/feedback', payload: { kind: 'idea', message: 'x', source: 'web' } })).statusCode).toBe(401);

    const admin = await createUser(t.db, { username: 'root', isAdmin: true });
    const adminHeaders = (await sessionFor(t.db, admin.id)).headers;
    expect((await t.app.inject({ method: 'GET', url: '/api/admin/feedback', headers })).statusCode).toBe(403);
    const [item] = (await t.app.inject({ method: 'GET', url: '/api/admin/feedback', headers: adminHeaders })).json();
    expect(item).toMatchObject({ kind: 'bug', hasLogs: true, system: { GPU: 'RTX 4070' }, user: { username: 'alice' }, resolvedAt: null });
    const logs = await t.app.inject({ method: 'GET', url: `/api/admin/feedback/${item.id}/logs`, headers: adminHeaders });
    expect(logs.headers['content-type']).toContain('text/plain');
    expect(logs.body).toBe('line 1\nline 2');
    const page = await t.app.inject({ method: 'GET', url: '/admin', headers: adminHeaders });
    expect(page.body).toContain('Hotkey does nothing in CS2');

    expect((await t.app.inject({ method: 'PATCH', url: `/api/admin/feedback/${item.id}`, headers: adminHeaders, payload: { resolved: true } })).statusCode).toBe(204);
    const [after] = (await t.app.inject({ method: 'GET', url: '/api/admin/feedback', headers: adminHeaders })).json();
    expect(after.resolvedAt).toBeTruthy();
    expect((await t.app.inject({ method: 'DELETE', url: `/api/admin/feedback/${item.id}`, headers: adminHeaders })).statusCode).toBe(204);
  });

  it('shows the account page', async () => {
    const res = await t.app.inject({ method: 'GET', url: '/account', headers });
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('Change password');
    expect(res.body).toContain('this device');
  });
});
