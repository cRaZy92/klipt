import { sql } from 'drizzle-orm';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { DEFAULT_LOOKS } from '@klipt/shared';
import { clips, userAchievements } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
let a: { id: string; h: Record<string, string> };
let b: { id: string; h: Record<string, string> };

async function mk(name: string) {
  const u = await createUser(t.db, { username: name });
  return { id: u.id, h: (await sessionFor(t.db, u.id)).headers };
}

const looks = (h: Record<string, string>, payload: unknown) => t.app.inject({ method: 'PUT', url: '/api/me/looks', headers: h, payload: payload as object });
const earn = (userId: string, achievementId: string) => t.db.insert(userAchievements).values({ userId, achievementId });

beforeEach(async () => {
  t = await makeTestApp();
  a = await mk('alice');
  b = await mk('bob');
});
afterEach(() => t.close());

describe('looks', () => {
  it('starts with the defaults', async () => {
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: a.h })).json().looks).toEqual(DEFAULT_LOOKS);
  });

  it('sets a free accent', async () => {
    const res = await looks(a.h, { accent: 'ocean' });
    expect(res.statusCode).toBe(200);
    expect(res.json().looks).toEqual({ ...DEFAULT_LOOKS, accent: 'ocean' });
    // Only the fields sent change.
    expect((await looks(a.h, { theme: 'klipt' })).json().looks.accent).toBe('ocean');
  });

  it('refuses a locked one and an unknown one', async () => {
    const locked = await looks(a.h, { theme: 'midnight' });
    expect(locked.statusCode).toBe(403);
    expect(locked.json().error).toBe('not_unlocked');
    expect((await looks(a.h, { accent: 'nope' })).statusCode).toBe(400);
    expect((await looks(a.h, { frame: 7 })).statusCode).toBe(400);
  });

  it('allows what an achievement unlocked', async () => {
    await earn(a.id, 'night-owl');
    expect((await looks(a.h, { theme: 'midnight' })).json().looks.theme).toBe('midnight');
  });

  it('shows the equipped frame to others, and none is always allowed', async () => {
    await earn(a.id, 'clips-10');
    expect((await looks(a.h, { frame: 'bronze' })).statusCode).toBe(200);
    await t.db.insert(clips).values({ id: 'clipAAAAAAAA', userId: a.id, title: 'x', durationMs: 1000, width: 1, height: 1, sizeBytes: 1 });
    const list = (await t.app.inject({ method: 'GET', url: '/api/clips', headers: b.h })).json();
    expect(list.items[0].owner.frame).toBe('bronze');
    expect((await looks(a.h, { frame: null, banner: null })).json().looks.frame).toBeNull();
  });

  it('reads garbage stored looks as the defaults', async () => {
    await t.db.execute(sql`update users set looks = '{"accent":"removed","theme":7,"frame":"gone"}'::jsonb where id = ${a.id}`);
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: a.h })).json().looks).toEqual(DEFAULT_LOOKS);
    await t.db.insert(clips).values({ id: 'clipAAAAAAAA', userId: a.id, title: 'x', durationMs: 1000, width: 1, height: 1, sizeBytes: 1 });
    expect((await t.app.inject({ method: 'GET', url: '/api/clips', headers: b.h })).json().items[0].owner.frame).toBeNull();
  });
});
