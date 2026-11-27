import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { invites } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
beforeEach(async () => {
  t = await makeTestApp();
});
afterEach(() => t.close());

describe('showcase page', () => {
  it('logged out: the showcase, with sign-up and the download', async () => {
    const res = await t.app.inject({ url: '/' });
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('Clips without bullshit.');
    expect(res.body).toContain('href="/signup"');
    expect(res.body).toContain('href="/get-app/windows"');
    expect(res.body).toContain('/static/showcase/hero.mp4');
    expect(res.body).not.toContain('invited you');
  });

  it('logged in: the feed', async () => {
    const u = await createUser(t.db, { username: 'alice' });
    const { cookie } = await sessionFor(t.db, u.id);
    const res = await t.app.inject({ url: '/', headers: { cookie } });
    expect(res.statusCode).toBe(200);
    expect(res.body).not.toContain('Clips without bullshit.');
  });

  it('with an active referral cookie: says who invited you', async () => {
    const u = await createUser(t.db, { username: 'alice' });
    await t.db.insert(invites).values({ code: 'refcode', createdBy: u.id, expiresAt: new Date(Date.now() + 1e6) });
    const res = await t.app.inject({ url: '/?ref=1', headers: { cookie: 'klipt_ref=refcode' } });
    expect(res.body).toContain('<strong>alice</strong> invited you');
  });

  it('after a used referral link: says so', async () => {
    const res = await t.app.inject({ url: '/?ref=invalid' });
    expect(res.body).toContain('already used or expired');
  });
});
