import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { clips } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
let alice: Record<string, string>;
let bob: Record<string, string>;

const insertClip = async (id: string, userId: string, visibility: 'feed' | 'link' = 'feed', title = id) => {
  await t.db.insert(clips).values({ id, userId, title, visibility, durationMs: 1000, width: 1, height: 1, sizeBytes: 1 });
};

beforeEach(async () => {
  t = await makeTestApp();
  const a = await createUser(t.db, { username: 'alice' });
  const b = await createUser(t.db, { username: 'bob' });
  alice = (await sessionFor(t.db, a.id)).headers;
  bob = (await sessionFor(t.db, b.id, 'desktop')).headers;
  await insertClip('aliceFeed001', a.id);
  await insertClip('aliceLink001', a.id, 'link');
  await insertClip('bobFeed00001', b.id);
});
afterEach(() => t.close());

const star = (id: string, h: Record<string, string>, on = true) =>
  t.app.inject({ method: on ? 'PUT' : 'DELETE', url: `/api/clips/${id}/favorite`, headers: h });
const list = async (h: Record<string, string>, query = '') =>
  (await t.app.inject({ url: `/api/clips?${query}`, headers: h })).json() as { items: { id: string; favorite: boolean }[] };

describe('favorites', () => {
  it('stars any clip you can open, per user, and lists them with favorites=1', async () => {
    expect((await star('aliceFeed001', bob)).statusCode).toBe(204);
    // A link-only clip: having the link is enough.
    expect((await star('aliceLink001', bob)).statusCode).toBe(204);
    // Twice is fine.
    expect((await star('aliceFeed001', bob)).statusCode).toBe(204);
    // Your own clip too.
    expect((await star('bobFeed00001', bob)).statusCode).toBe(204);

    const feed = await list(bob, 'scope=feed');
    expect(feed.items.find((c) => c.id === 'aliceFeed001')?.favorite).toBe(true);
    expect((await list(alice, 'scope=feed')).items.every((c) => !c.favorite)).toBe(true);

    const favs = await list(bob, 'scope=feed&favorites=1');
    expect(favs.items.map((c) => c.id).sort()).toEqual(['aliceFeed001', 'aliceLink001', 'bobFeed00001']);
    expect((await list(alice, 'favorites=1')).items).toEqual([]);

    const one = await t.app.inject({ url: '/api/clips/aliceFeed001', headers: bob });
    expect(one.json().favorite).toBe(true);

    expect((await star('aliceFeed001', bob, false)).statusCode).toBe(204);
    expect((await list(bob, 'favorites=1')).items.map((c) => c.id).sort()).toEqual(['aliceLink001', 'bobFeed00001']);
  });

  it('needs a login and an existing clip', async () => {
    expect((await star('aliceFeed001', {})).statusCode).toBe(401);
    expect((await star('nope00000000', bob)).statusCode).toBe(404);
    expect((await star('bad', bob)).statusCode).toBe(404);
  });

  it('the website: a Favorites chip, a star on cards, a Favorite button on the share page', async () => {
    await star('aliceFeed001', alice);
    const feed = await t.app.inject({ url: '/?fav=1', headers: alice });
    expect(feed.body).toContain('★ Favorites');
    expect(feed.body).toContain('href="/c/aliceFeed001"');
    expect(feed.body).not.toContain('href="/c/bobFeed00001"');
    expect(feed.body).toContain('class="fav-mark"');

    const share = await t.app.inject({ url: '/c/aliceFeed001', headers: alice });
    expect(share.body).toContain('data-action="favorite" aria-pressed="true"');
    const other = await t.app.inject({ url: '/c/bobFeed00001', headers: alice });
    expect(other.body).toContain('data-action="favorite" aria-pressed="false"');
    // Logged out: no button.
    expect((await t.app.inject({ url: '/c/bobFeed00001' })).body).not.toContain('data-action="favorite"');
  });
});
