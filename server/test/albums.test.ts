import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { MAX_ALBUM_CLIPS, type Album, type AlbumSummary } from '@klipt/shared';
import { albumClips, clips } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
let alice: Record<string, string>;
let bob: Record<string, string>;
let aliceId: string;

const insertClip = async (id: string, userId: string, visibility: 'feed' | 'link' = 'feed') => {
  await t.db.insert(clips).values({ id, userId, title: id, visibility, durationMs: 1000, width: 1, height: 1, sizeBytes: 1 });
};

beforeEach(async () => {
  t = await makeTestApp();
  const a = await createUser(t.db, { username: 'alice' });
  const b = await createUser(t.db, { username: 'bob' });
  aliceId = a.id;
  alice = (await sessionFor(t.db, a.id)).headers;
  bob = (await sessionFor(t.db, b.id, 'desktop')).headers;
  await insertClip('aliceFeed001', a.id);
  await insertClip('aliceLink001', a.id, 'link');
  await insertClip('bobFeed00001', b.id);
});
afterEach(() => t.close());

const req = (method: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE', url: string, headers: Record<string, string>, payload?: object) =>
  t.app.inject({ method, url, headers, ...(payload ? { payload } : {}) });

async function create(headers: Record<string, string>, title: string, clipIds?: string[]): Promise<Album> {
  const res = await req('POST', '/api/albums', headers, { title, ...(clipIds ? { clipIds } : {}) });
  expect(res.statusCode).toBe(201);
  return res.json();
}

describe('albums', () => {
  it('gathers anyone’s clips, in order, under one link', async () => {
    const album = await create(alice, '  Best of the tournament ', ['bobFeed00001', 'aliceFeed001']);
    expect(album).toMatchObject({
      title: 'Best of the tournament',
      owner: { username: 'alice' },
      clipCount: 2,
      shareUrl: `http://localhost/a/${album.id}`,
      coverUrl: 'http://localhost/media/bobFeed00001.jpg',
    });
    expect(album.clips.map((c) => c.id)).toEqual(['bobFeed00001', 'aliceFeed001']);

    // A link-only clip goes in too (having its link is enough), at the end; twice changes nothing.
    expect((await req('PUT', `/api/albums/${album.id}/clips/aliceLink001`, alice)).statusCode).toBe(204);
    expect((await req('PUT', `/api/albums/${album.id}/clips/aliceLink001`, alice)).statusCode).toBe(204);
    const got: Album = (await req('GET', `/api/albums/${album.id}`, bob)).json();
    expect(got.clips.map((c) => c.id)).toEqual(['bobFeed00001', 'aliceFeed001', 'aliceLink001']);

    const reordered = await req('PATCH', `/api/albums/${album.id}`, alice, {
      title: 'Finals',
      clipIds: ['aliceLink001', 'bobFeed00001', 'aliceFeed001'],
    });
    expect(reordered.statusCode).toBe(200);
    expect(reordered.json()).toMatchObject({ title: 'Finals', coverUrl: 'http://localhost/media/aliceLink001.jpg' });
    expect((reordered.json() as Album).clips.map((c) => c.id)).toEqual(['aliceLink001', 'bobFeed00001', 'aliceFeed001']);

    expect((await req('DELETE', `/api/albums/${album.id}/clips/bobFeed00001`, alice)).statusCode).toBe(204);
    // A deleted clip leaves the album.
    await t.db.delete(clips).where(eq(clips.id, 'aliceLink001'));
    const after: Album = (await req('GET', `/api/albums/${album.id}`, alice)).json();
    expect(after.clips.map((c) => c.id)).toEqual(['aliceFeed001']);
    expect(after.clipCount).toBe(1);
  });

  it('lists your own albums, newest change first, and which hold a clip', async () => {
    const one = await create(alice, 'One', ['aliceFeed001']);
    const two = await create(alice, 'Two');
    await create(bob, 'Bob’s');
    await req('PUT', `/api/albums/${one.id}/clips/bobFeed00001`, alice);
    const list: AlbumSummary[] = (await req('GET', '/api/albums?clip=aliceFeed001', alice)).json();
    expect(list.map((a) => [a.title, a.clipCount, a.hasClip])).toEqual([
      ['One', 2, true],
      ['Two', 0, false],
    ]);
    expect(list[1]!.coverUrl).toBeNull();
    const plain: AlbumSummary[] = (await req('GET', '/api/albums', alice)).json();
    expect(plain.every((a) => a.hasClip === undefined)).toBe(true);
    expect(two.id).not.toBe(one.id);
  });

  it('only lets the owner change an album', async () => {
    const album = await create(alice, 'Mine', ['aliceFeed001']);
    for (const res of [
      await req('PATCH', `/api/albums/${album.id}`, bob, { title: 'Hacked' }),
      await req('PUT', `/api/albums/${album.id}/clips/bobFeed00001`, bob),
      await req('DELETE', `/api/albums/${album.id}/clips/aliceFeed001`, bob),
      await req('DELETE', `/api/albums/${album.id}`, bob),
    ]) {
      expect(res.statusCode).toBe(403);
    }
    expect((await req('GET', `/api/albums/${album.id}`, {})).statusCode).toBe(401);
    expect((await req('DELETE', `/api/albums/${album.id}`, alice)).statusCode).toBe(204);
    expect((await req('GET', `/api/albums/${album.id}`, alice)).statusCode).toBe(404);
  });

  it('refuses bad input', async () => {
    const album = await create(alice, 'A', ['aliceFeed001']);
    expect((await req('POST', '/api/albums', alice, { title: '   ' })).statusCode).toBe(400);
    expect((await req('POST', '/api/albums', alice, { title: 'x'.repeat(101) })).statusCode).toBe(400);
    expect((await req('POST', '/api/albums', alice, { title: 'A', clipIds: ['nope00000000'] })).statusCode).toBe(400);
    expect((await req('PUT', `/api/albums/${album.id}/clips/nope00000000`, alice)).statusCode).toBe(404);
    expect((await req('PUT', `/api/albums/nope00000000/clips/aliceFeed001`, alice)).statusCode).toBe(404);
    // A new order must hold exactly the album's clips.
    expect((await req('PATCH', `/api/albums/${album.id}`, alice, { clipIds: [] })).statusCode).toBe(409);
    expect((await req('PATCH', `/api/albums/${album.id}`, alice, { clipIds: ['aliceFeed001', 'aliceFeed001'] })).statusCode).toBe(409);
    expect((await req('PATCH', `/api/albums/${album.id}`, alice, { clipIds: ['bobFeed00001'] })).statusCode).toBe(409);
  });

  it(`holds up to ${MAX_ALBUM_CLIPS} clips`, async () => {
    const album = await create(alice, 'Big');
    const ids = Array.from({ length: MAX_ALBUM_CLIPS }, (_, i) => `many${String(i).padStart(8, '0')}`);
    await t.db.insert(clips).values(ids.map((id) => ({ id, userId: aliceId, title: id, durationMs: 1, width: 1, height: 1, sizeBytes: 1 })));
    await t.db.insert(albumClips).values(ids.map((clipId, position) => ({ albumId: album.id, clipId, position })));
    const res = await req('PUT', `/api/albums/${album.id}/clips/aliceFeed001`, alice);
    expect(res.statusCode).toBe(409);
    expect(res.json().error).toBe('album_full');
  });
});

describe('album pages', () => {
  it('lists your albums on /albums, and needs a login', async () => {
    await create(alice, 'Finals <3', ['aliceFeed001']);
    const page = await req('GET', '/albums', alice);
    expect(page.statusCode).toBe(200);
    expect(page.body).toContain('Finals &lt;3');
    expect(page.body).toContain('1 clip');
    expect((await req('GET', '/albums', {})).statusCode).toBe(302);
  });

  it('plays the album to anyone with the link, the picked clip or the first', async () => {
    const album = await create(alice, 'Finals', ['aliceLink001', 'bobFeed00001']);
    const page = await req('GET', `/a/${album.id}`, {});
    expect(page.statusCode).toBe(200);
    expect(page.body).toContain('<meta property="og:title" content="Finals" />');
    expect(page.body).toContain('2 clips · album by alice');
    expect(page.body).toContain('src="http://localhost/media/aliceLink001.mp4"');
    expect(page.body).toContain('1 of 2');
    // Owner tools only for the owner.
    expect(page.body).not.toContain('data-action="album-remove"');

    const second = await req('GET', `/a/${album.id}?c=bobFeed00001`, alice);
    expect(second.body).toContain('src="http://localhost/media/bobFeed00001.mp4"');
    expect(second.body).toContain('2 of 2');
    expect(second.body).toContain('data-action="album-remove"');

    expect((await req('GET', '/a/nope00000000', {})).statusCode).toBe(404);
  });

  it('offers Add to album on a clip’s page when logged in', async () => {
    expect((await req('GET', '/c/aliceFeed001', bob)).body).toContain('data-action="album-pick"');
    expect((await req('GET', '/c/aliceFeed001', {})).body).not.toContain('data-action="album-pick"');
  });
});
