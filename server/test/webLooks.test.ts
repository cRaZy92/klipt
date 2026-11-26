import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { ACCENTS } from '@klipt/shared';
import { clips, userAchievements, users } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;

const get = (url: string, cookie?: string) => t.app.inject({ method: 'GET', url, headers: cookie ? { cookie } : {} });

async function mk(name: string) {
  const u = await createUser(t.db, { username: name });
  return { u, cookie: (await sessionFor(t.db, u.id)).cookie };
}

beforeEach(async () => {
  t = await makeTestApp();
});
afterEach(() => t.close());

describe('website looks and achievements', () => {
  it('uses your accent when logged in, the stylesheet defaults when not', async () => {
    const a = await mk('alice');
    await t.app.inject({ method: 'PUT', url: '/api/me/looks', headers: { cookie: a.cookie }, payload: { accent: 'ocean' } });
    const page = await get('/', a.cookie);
    expect(page.body).toContain(`--accent:${ACCENTS.find((x) => x.id === 'ocean')!.color}`);
    expect((await get('/login')).body).not.toContain(':root{');
  });

  it('shows the picture, frame, banner and badges on a profile', async () => {
    const a = await mk('alice');
    const b = await mk('bob');
    await t.db.update(users).set({ avatarVersion: 1, looks: { accent: 'klipt', theme: 'klipt', frame: 'bronze', banner: 'curator' } }).where(eq(users.id, a.u.id));
    await t.db.insert(userAchievements).values([
      { userId: a.u.id, achievementId: 'first-upload' },
      { userId: a.u.id, achievementId: 'night-owl' },
    ]);
    const page = (await get('/u/alice', b.cookie)).body;
    expect(page).toContain(`src="/avatars/${a.u.id}.jpg?v=1"`);
    expect(page).toContain('frame-bronze');
    expect(page).toContain('banner-curator');
    expect(page).toMatch(/class="badge-medal"[^>]*title="Going live: Uploaded your first clip"/);
    expect(page).toContain('Night owl');
  });

  it('feed cards and the share page draw the owner with picture and frame', async () => {
    const a = await mk('alice');
    const b = await mk('bob');
    await t.db.update(users).set({ avatarVersion: 2, looks: { frame: 'gold' } }).where(eq(users.id, a.u.id));
    await t.db.insert(clips).values({ id: 'webclip00001', userId: a.u.id, title: 'x', durationMs: 1000, width: 1, height: 1, sizeBytes: 1 });
    const feed = (await get('/', b.cookie)).body;
    expect(feed).toContain(`/avatars/${a.u.id}.jpg?v=2`);
    expect(feed).toContain('frame-gold');
    expect((await get('/c/webclip00001', b.cookie)).body).toContain('frame-gold');
  });

  it('marks new achievements on the account button until you open your profile', async () => {
    const a = await mk('alice');
    await t.db.insert(userAchievements).values({ userId: a.u.id, achievementId: 'first-upload' });
    expect((await get('/', a.cookie)).body).toContain('new-dot');
    await get('/u/alice', a.cookie);
    expect((await get('/', a.cookie)).body).not.toContain('new-dot');
  });

  it('someone else opening your profile does not mark yours seen', async () => {
    const a = await mk('alice');
    const b = await mk('bob');
    await t.db.insert(userAchievements).values({ userId: a.u.id, achievementId: 'first-upload' });
    await get('/u/alice', b.cookie);
    expect((await get('/', a.cookie)).body).toContain('new-dot');
  });

  it('lists every accent on the account page, locked ones disabled', async () => {
    const a = await mk('alice');
    let page = (await get('/account', a.cookie)).body;
    for (const accent of ACCENTS) expect(page).toContain(`value="${accent.id}"`);
    expect(page).toMatch(/name="accent" value="gold"[^>]*disabled/);
    expect(page).toContain('Unlocks with Variety');
    await t.db.insert(userAchievements).values({ userId: a.u.id, achievementId: 'games-5' });
    page = (await get('/account', a.cookie)).body;
    expect(page).not.toMatch(/name="accent" value="gold"[^>]*disabled/);
    expect(page).toContain('data-form="avatar"');
  });
});
