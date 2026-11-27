import { mkdtemp, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { clips, clipTags, users } from '../src/db/schema.js';
import { listClips } from '../src/clips/repo.js';
import { makeFriends, requestFriend } from '../src/friends/repo.js';
import { createUser, makeFixtureVideo, makeTestApp, sessionFor, tusUpload, type TestApp } from './helpers.js';

type Vis = 'link' | 'friends' | 'public';
const LEVELS: Vis[] = ['link', 'friends', 'public'];

let t: TestApp;
const id: Record<string, string> = {};
const h: Record<string, Record<string, string>> = {};

/** Clip ids are `<user><level>` padded to 12, e.g. `meLink000000`. */
const clipId = (user: string, v: Vis) => `${user}${v[0]!.toUpperCase()}${v.slice(1)}`.padEnd(12, '0').slice(0, 12);

beforeEach(async () => {
  t = await makeTestApp();
  for (const name of ['me', 'friend', 'pending', 'stranger', 'pub']) {
    const u = await createUser(t.db, { username: name });
    id[name] = u.id;
    h[name] = (await sessionFor(t.db, u.id)).headers;
    for (const v of LEVELS) {
      await t.db.insert(clips).values({ id: clipId(name, v), userId: u.id, title: `${name} ${v}`, gameName: `${name}-game-${v}`, visibility: v, durationMs: 1000, width: 1, height: 1, sizeBytes: 1 });
    }
  }
  await makeFriends(t.db, id.me!, id.friend!);
  await requestFriend(t.db, id.me!, id.pending!);
  await t.db.update(users).set({ profileVisibility: 'public' }).where(eq(users.id, id.pub!));
});
afterEach(() => t.close());

const ids = async (viewer: string, query: string) => {
  const res = await t.app.inject({ url: `/api/clips?${query}`, headers: h[viewer] });
  expect(res.statusCode).toBe(200);
  return ((res.json() as { items: { id: string }[] }).items.map((c) => c.id)).sort();
};
const all = (user: string, levels: Vis[] = LEVELS) => levels.map((v) => clipId(user, v));

describe('who sees what', () => {
  it("feed shows own and friends' non-link clips only", async () => {
    expect(await ids('me', 'scope=feed')).toEqual([...all('me'), ...all('friend', ['friends', 'public'])].sort());
  });

  it('pending grants nothing', async () => {
    expect(await ids('me', 'scope=feed')).not.toContain(clipId('pending', 'friends'));
    expect(await ids('pending', 'scope=feed')).toEqual(all('pending').sort());
    expect(await ids('pending', 'scope=user&username=me')).toEqual([clipId('me', 'public')]);
  });

  it('public clips of non-friends are not on the feed', async () => {
    expect(await ids('me', 'scope=feed')).not.toContain(clipId('pub', 'public'));
  });

  it('profiles: own shows all, a friend shows friends+public, a stranger only public', async () => {
    expect(await ids('me', 'scope=user&username=me')).toEqual(all('me').sort());
    expect(await ids('friend', 'scope=user&username=me')).toEqual(all('me', ['friends', 'public']).sort());
    expect(await ids('stranger', 'scope=user&username=me')).toEqual([clipId('me', 'public')]);
  });

  it('stranger sees nothing', async () => {
    expect(await ids('stranger', 'scope=feed')).toEqual(all('stranger').sort());
    expect(await ids('stranger', 'scope=user&username=me&q=me')).toEqual([clipId('me', 'public')]);
    const games = (await t.app.inject({ url: '/api/games?scope=user&username=me', headers: h.stranger })).json() as { gameName: string }[];
    expect(games.map((g) => g.gameName)).toEqual(['me-game-public']);
    const people = (await t.app.inject({ url: '/api/users', headers: h.stranger })).json();
    expect(people).toEqual([]);

    // Starred while friends, then unfriended: it drops out of favorites.
    await makeFriends(t.db, id.stranger!, id.me!);
    expect((await t.app.inject({ method: 'PUT', url: `/api/clips/${clipId('me', 'friends')}/favorite`, headers: h.stranger })).statusCode).toBe(204);
    expect((await t.app.inject({ method: 'PUT', url: `/api/clips/${clipId('me', 'link')}/favorite`, headers: h.stranger })).statusCode).toBe(204);
    expect(await ids('stranger', 'favorites=1')).toEqual([clipId('me', 'friends'), clipId('me', 'link')].sort());
    await t.app.inject({ method: 'DELETE', url: '/api/friends/me', headers: h.stranger });
    // A link-only clip stays: starring it needed the link anyway.
    expect(await ids('stranger', 'favorites=1')).toEqual([clipId('me', 'link')]);
  });

  it('users list is your friends', async () => {
    const people = (await t.app.inject({ url: '/api/users', headers: h.me })).json() as { username: string }[];
    expect(people.map((p) => p.username)).toEqual(['friend']);
  });

  it('logged out sees public clips of any profile', async () => {
    const { items } = await listClips(t.db, null, { scope: 'user', username: 'me' });
    expect(items.map((i) => i.clip.id)).toEqual([clipId('me', 'public')]);
  });

  it('tagged clip shows on profile only when uploader and tagged are friends', async () => {
    // pub tags me in pub's public clip, without being my friend: not on my profile.
    await t.db.insert(clipTags).values({ clipId: clipId('pub', 'public'), userId: id.me! });
    expect(await ids('friend', 'scope=user&username=me')).not.toContain(clipId('pub', 'public'));
    // friend tags me in friend's friends clip: on my profile, for viewers who can see that clip.
    await t.db.insert(clipTags).values({ clipId: clipId('friend', 'friends'), userId: id.me! });
    expect(await ids('me', 'scope=user&username=me')).toContain(clipId('friend', 'friends'));
    expect(await ids('stranger', 'scope=user&username=me')).not.toContain(clipId('friend', 'friends'));
  });

  it('tagging a non-friend is refused', async () => {
    const res = await t.app.inject({ method: 'PATCH', url: `/api/clips/${clipId('me', 'public')}`, headers: h.me, payload: { tagged: ['stranger'] } });
    expect(res.statusCode).toBe(400);
    expect(res.json().message).toBe('No user called stranger.');
    const ok = await t.app.inject({ method: 'PATCH', url: `/api/clips/${clipId('me', 'public')}`, headers: h.me, payload: { tagged: ['friend'] } });
    expect(ok.statusCode).toBe(200);
  });

  it('feed alias: an old app saying feed gets a friends clip', async () => {
    const res = await t.app.inject({ method: 'PATCH', url: `/api/clips/${clipId('me', 'link')}`, headers: h.me, payload: { visibility: 'feed' } });
    expect(res.statusCode).toBe(200);
    expect(res.json().visibility).toBe('friends');
  });
});

describe('older desktop apps', () => {
  it("see 'feed' for Friends and Public clips; new apps and the website see the real values", async () => {
    const desk = await sessionFor(t.db, id.me!, 'desktop');
    const vis = async (headers: Record<string, string>) =>
      ((await t.app.inject({ url: '/api/clips?scope=mine', headers })).json() as { items: { id: string; visibility: string }[] }).items
        .map((c) => c.visibility)
        .sort();
    expect(await vis(desk.headers)).toEqual(['feed', 'feed', 'link']);
    expect(await vis({ ...desk.headers, 'x-klipt-api': '2' })).toEqual(['friends', 'link', 'public']);
    expect(await vis(h.me!)).toEqual(['friends', 'link', 'public']);
    const one = await t.app.inject({ url: `/api/clips/${clipId('me', 'public')}`, headers: desk.headers });
    expect(one.json().visibility).toBe('feed');
  });
});

describe('upload visibility', () => {
  it('finalize: feed means friends, and no visibility follows the profile', async () => {
    const s = await sessionFor(t.db, id.pub!, 'desktop');
    const dir = await mkdtemp(join(tmpdir(), 'klipt-fx-'));
    const fixture = await readFile(await makeFixtureVideo(dir));
    const finalize = async (extra: Record<string, unknown>, old = false) => {
      const { uploadId } = await tusUpload(t.app, s.token, fixture);
      const headers = old ? s.headers : { ...s.headers, 'x-klipt-api': '2' };
      const res = await t.app.inject({ method: 'POST', url: '/api/clips', headers, payload: { uploadId, title: 'x', ...extra } });
      expect(res.statusCode).toBe(201);
      return res.json().visibility as string;
    };
    expect(await finalize({})).toBe('public');
    expect(await finalize({ visibility: 'link' })).toBe('link');
    expect(await finalize({ visibility: 'friends' })).toBe('friends');
    // Older apps only understand feed/link in this answer: they get 'feed' back, the clip is Friends.
    expect(await finalize({ visibility: 'feed' }, true)).toBe('feed');
    const rows = await t.db.select({ v: clips.visibility }).from(clips).where(eq(clips.userId, id.pub!));
    expect(rows.map((r) => r.v)).not.toContain('feed');
    expect(rows.filter((r) => r.v === 'friends')).toHaveLength(3);
  });
});
