import { describe, expect, it, vi } from 'vitest';
import { createLatestInstaller, RELEASES_PAGE } from '../src/web/latestInstaller.js';

const SETUP = 'https://github.com/cRaZy92/klipt/releases/download/v0.1.42/Klipt_0.1.42_x64-setup.exe';

const latestJson = (url: unknown) =>
  new Response(JSON.stringify({ version: '0.1.42', platforms: { 'windows-x86_64': { signature: 'x', url } } }), {
    headers: { 'content-type': 'application/json' },
  });

describe('createLatestInstaller', () => {
  it("returns the installer named in the latest release's latest.json and caches it", async () => {
    let now = 0;
    const fetch = vi.fn(async (_url: string) => latestJson(SETUP));
    const latest = createLatestInstaller({ fetch, now: () => now });
    expect(await latest()).toBe(SETUP);
    expect(await latest()).toBe(SETUP);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch.mock.calls[0]![0]).toBe('https://github.com/cRaZy92/klipt/releases/latest/download/latest.json');
    now += 11 * 60 * 1000;
    await latest();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it('falls back to the releases page on errors or a URL from elsewhere, and retries soon', async () => {
    let now = 0;
    const replies = [
      () => Promise.reject(new Error('offline')),
      async () => new Response('nope', { status: 404 }),
      async () => latestJson('https://evil.example/Klipt-setup.exe'),
      async () => latestJson(SETUP),
    ];
    const fetch = vi.fn((_url: string) => replies.shift()!());
    const latest = createLatestInstaller({ fetch, now: () => now });
    for (let i = 0; i < 3; i++) {
      expect(await latest()).toBe(RELEASES_PAGE);
      now += 61 * 1000;
    }
    expect(await latest()).toBe(SETUP);
  });
});
