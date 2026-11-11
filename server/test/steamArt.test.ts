import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createSteamArt, parseAppId } from '../src/web/steamArt.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

// What store.steampowered.com/api/appdetails returned for WARDOGS on 2026-11-11 (trimmed).
const WARDOGS_ART =
  'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/1867240/59d4daf753bd5d982e6675f7eee363bc817c574e/header.jpg?t=1791928607';

function steamReply(appId: number, data: unknown, success = true) {
  return new Response(JSON.stringify({ [appId]: success ? { success, data } : { success } }), {
    headers: { 'content-type': 'application/json' },
  });
}

describe('parseAppId', () => {
  it('accepts Steam app ids only', () => {
    expect(parseAppId('1867240')).toBe(1867240);
    expect(parseAppId('0')).toBeNull();
    expect(parseAppId('01')).toBeNull();
    expect(parseAppId('12a')).toBeNull();
    expect(parseAppId('99999999999')).toBeNull();
    expect(parseAppId('4294967296')).toBeNull();
  });
});

describe('createSteamArt', () => {
  it('returns the hashed header URL from appdetails and caches it', async () => {
    const fetch = vi.fn(async (_url: string) => steamReply(1867240, { header_image: WARDOGS_ART }));
    const art = createSteamArt({ fetch });
    expect(await art(1867240)).toBe(WARDOGS_ART);
    expect(await art(1867240)).toBe(WARDOGS_ART);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch.mock.calls[0]?.[0]).toBe('https://store.steampowered.com/api/appdetails?appids=1867240&filters=basic');
  });

  it('gives null for unknown apps, bad replies, network errors and non-Steam hosts', async () => {
    expect(await createSteamArt({ fetch: async () => steamReply(1, null, false) })(1)).toBeNull();
    expect(await createSteamArt({ fetch: async () => new Response('busy', { status: 503 }) })(1)).toBeNull();
    expect(await createSteamArt({ fetch: async () => Promise.reject(new Error('offline')) })(1)).toBeNull();
    expect(await createSteamArt({ fetch: async () => steamReply(1, { header_image: 'https://evil.example/x.jpg' }) })(1)).toBeNull();
    expect(await createSteamArt({ fetch: async () => steamReply(1, { header_image: 'http://cdn.steamstatic.com/x.jpg' }) })(1)).toBeNull();
  });

  it('retries a miss after 10 minutes, keeps a hit for a day', async () => {
    let now = 0;
    let reply: unknown = null;
    const fetch = vi.fn(async () => steamReply(5, reply === null ? null : { header_image: reply }, reply !== null));
    const art = createSteamArt({ fetch, now: () => now });

    expect(await art(5)).toBeNull();
    now = 9 * 60_000;
    expect(await art(5)).toBeNull();
    expect(fetch).toHaveBeenCalledTimes(1);

    reply = WARDOGS_ART;
    now = 11 * 60_000;
    expect(await art(5)).toBe(WARDOGS_ART);
    now += 23 * 3_600_000;
    expect(await art(5)).toBe(WARDOGS_ART);
    expect(fetch).toHaveBeenCalledTimes(2);
  });
});

describe('GET /steam-art/:appId', () => {
  let t: TestApp;
  beforeEach(async () => {
    t = await makeTestApp();
    vi.stubGlobal('fetch', vi.fn(async () => steamReply(1867240, { header_image: WARDOGS_ART })));
  });
  afterEach(async () => {
    vi.unstubAllGlobals();
    await t.close();
  });

  const get = (url: string, cookie?: string) => t.app.inject({ method: 'GET', url, headers: cookie ? { cookie } : {} });

  it('redirects a logged-in user to the Steam art', async () => {
    const u = await createUser(t.db, { username: 'alice' });
    const { cookie } = await sessionFor(t.db, u.id);
    const res = await get('/steam-art/1867240', cookie);
    expect(res.statusCode).toBe(302);
    expect(res.headers.location).toBe(WARDOGS_ART);
    expect(res.headers['cache-control']).toBe('private, max-age=86400');
  });

  it('is 404 when logged out or for a bad id, without asking Steam', async () => {
    const u = await createUser(t.db, { username: 'alice' });
    const { cookie } = await sessionFor(t.db, u.id);
    expect((await get('/steam-art/1867240')).statusCode).toBe(404);
    expect((await get('/steam-art/abc', cookie)).statusCode).toBe(404);
    expect(fetch).not.toHaveBeenCalled();
  });
});
