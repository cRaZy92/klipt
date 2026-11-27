import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { clips, users } from '../src/db/schema.js';
import { makeFriends } from '../src/friends/repo.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
let me: { id: string; cookie: string };
let friend: { id: string; cookie: string };
let stranger: { id: string; cookie: string };

const mk = async (name: string) => {
  const u = await createUser(t.db, { username: name });
  return { id: u.id, cookie: (await sessionFor(t.db, u.id)).cookie };
};
const get = (url: string, cookie?: string) => t.app.inject({ method: 'GET', url, headers: cookie ? { cookie } : {} });

beforeEach(async () => {
  t = await makeTestApp();
  me = await mk('me');
  friend = await mk('friend');
  stranger = await mk('stranger');
  await makeFriends(t.db, me.id, friend.id);
  for (const v of ['link', 'friends', 'public'] as const) {
    await t.db.insert(clips).values({ id: `me${v}`.padEnd(12, '0'), userId: me.id, title: `Title ${v} clip`, visibility: v, durationMs: 1000, width: 1, height: 1, sizeBytes: 1 });
  }
});
afterEach(() => t.close());

describe('profile visibility', () => {
  it('PATCH /api/me sets it; Me returns it', async () => {
    const headers = { cookie: me.cookie };
    expect((await t.app.inject({ url: '/api/me', headers })).json().profileVisibility).toBe('friends');
    const res = await t.app.inject({ method: 'PATCH', url: '/api/me', headers, payload: { profileVisibility: 'public' } });
    expect(res.statusCode).toBe(200);
    expect(res.json().profileVisibility).toBe('public');
    const bad = await t.app.inject({ method: 'PATCH', url: '/api/me', headers, payload: { profileVisibility: 'everyone' } });
    expect(bad.statusCode).toBe(400);
  });
});

describe('profile pages', () => {
  it('a Friends profile, logged out: public clips only, no stats, Log in to add', async () => {
    const res = await get('/u/me');
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('Title public clip');
    expect(res.body).not.toContain('Title friends clip');
    expect(res.body).not.toContain('Title link clip');
    expect(res.body).not.toContain('profile-stats');
    expect(res.body).toContain('Log in to add');
  });

  it('a Friends profile, seen by a stranger: Add friend', async () => {
    const res = await get('/u/me', stranger.cookie);
    expect(res.body).toContain('Title public clip');
    expect(res.body).not.toContain('Title friends clip');
    expect(res.body).toContain('data-action="friend-add"');
    expect(res.body).not.toContain('profile-stats');
  });

  it('a Public profile, logged out: stats too', async () => {
    await t.db.update(users).set({ profileVisibility: 'public' }).where(eq(users.id, me.id));
    const res = await get('/u/me');
    expect(res.body).toContain('profile-stats');
    expect(res.body).toContain('Title public clip');
    expect(res.body).not.toContain('Title friends clip');
  });

  it("a friend's profile shows Friends and their friends clips", async () => {
    const res = await get('/u/me', friend.cookie);
    expect(res.body).toContain('Title friends clip');
    expect(res.body).toContain('data-friend-state="friends"');
  });

  it('your own profile has no friend button', async () => {
    const res = await get('/u/me', me.cookie);
    expect(res.body).toContain('Title link clip');
    expect(res.body).not.toContain('data-friend-state');
  });

  it('an incoming request shows Accept', async () => {
    await t.app.inject({ method: 'POST', url: '/api/friends/stranger', headers: { cookie: me.cookie } });
    const res = await get('/u/me', stranger.cookie);
    expect(res.body).toContain('data-friend-state="incoming"');
    expect(res.body).toContain('Accept');
  });
});

describe('friends page', () => {
  it('needs a login', async () => {
    const res = await get('/friends');
    expect(res.statusCode).toBe(302);
    expect(res.headers.location).toBe('/login?next=%2Ffriends');
  });

  it('lists requests, friends and referral links; the top bar counts requests', async () => {
    await t.app.inject({ method: 'POST', url: '/api/friends/me', headers: { cookie: stranger.cookie } });
    await t.app.inject({ method: 'POST', url: '/api/referrals', headers: { cookie: me.cookie } });
    const res = await get('/friends', me.cookie);
    expect(res.statusCode).toBe(200);
    expect(res.body).toMatch(/data-section="incoming"[\s\S]*stranger[\s\S]*data-action="friend-add"[\s\S]*Accept/);
    expect(res.body).toMatch(/data-section="friends"[\s\S]*friend/);
    expect(res.body).toContain('data-action="make-referral"');
    expect(res.body).toMatch(/http:\/\/localhost\/r\/[A-Za-z0-9_-]{16}/);
    expect(res.body).toMatch(/href="\/friends"[^>]*>Friends<span class="badge accent nav-count">1<\/span>/);
  });

  it('says when your own or a used link was opened', async () => {
    expect((await get('/friends?ref=own', me.cookie)).body).toContain('your own link');
    expect((await get('/friends?ref=invalid', me.cookie)).body).toContain('already used or expired');
  });
});

describe('install guide', () => {
  it("doesn't ask for a server address anymore", async () => {
    const res = await get('/get-app', me.cookie);
    expect(res.statusCode).toBe(200);
    expect(res.body).not.toContain("server's address");
    expect(res.body).toContain('Open Klipt and log in');
  });
});

describe('GET /api/users/:username/profile (hover card)', () => {
  it('a Friends profile, logged out: name and picture only', async () => {
    const res = await get('/api/users/me/profile');
    expect(res.statusCode).toBe(200);
    const p = res.json();
    expect(p.user.username).toBe('me');
    expect(p.full).toBe(false);
    expect(p.stats).toBeNull();
    expect(p.badges).toEqual([]);
    expect(p.friendState).toBeNull();
    expect(p.profileUrl).toBe('http://localhost/u/me');
    expect(Date.parse(p.joinedAt)).not.toBeNaN();
  });

  it('a friend sees stats over the clips they may see, and Friends', async () => {
    const p = (await get('/api/users/ME/profile', friend.cookie)).json();
    expect(p.full).toBe(true);
    expect(p.friendState).toBe('friends');
    expect(p.stats.clips).toBe(2); // friends + public, not the link-only one
  });

  it('a stranger on a Friends profile: Add friend, nothing else', async () => {
    const p = (await get('/api/users/me/profile', stranger.cookie)).json();
    expect(p.full).toBe(false);
    expect(p.friendState).toBe('none');
  });

  it('unknown or disabled users are 404', async () => {
    expect((await get('/api/users/nobody/profile')).statusCode).toBe(404);
    await t.db.update(users).set({ disabledAt: new Date() }).where(eq(users.id, stranger.id));
    expect((await get('/api/users/stranger/profile')).statusCode).toBe(404);
  });
});

describe('clip cards link the owner', () => {
  it("the owner's picture and name open their profile", async () => {
    const res = await get('/u/me', friend.cookie);
    expect(res.body).toContain('class="owner person-link" href="/u/me"');
    expect(res.body).toContain('class="card-title card-link" href="/c/mepublic0000"');
  });
});

describe('join date', () => {
  it('the profile page says when they joined', async () => {
    await t.db.update(users).set({ createdAt: new Date('2026-03-15T12:00:00Z') }).where(eq(users.id, me.id));
    expect((await get('/u/me')).body).toContain('Joined March 2026');
  });
});
