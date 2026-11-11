import { afterEach, describe, expect, it } from 'vitest';
import { makeTestApp, type TestApp } from './helpers.js';

describe('request logs', () => {
  let t: TestApp;
  afterEach(async () => {
    await t.close();
  });

  it('show route patterns, never share ids or invite codes', async () => {
    const lines: string[] = [];
    t = await makeTestApp({ logStream: { write: (line) => void lines.push(line) } });
    await t.app.inject({ method: 'GET', url: '/c/SecretShareId42' });
    await t.app.inject({ method: 'GET', url: '/invite/SecretInviteCode7?x=1' });
    await t.app.inject({ method: 'GET', url: '/media/SecretShareId42.mp4' });
    await t.app.inject({ method: 'GET', url: '/no/such/SecretPath' });

    const log = lines.join('');
    expect(log).toContain('"url":"/c/:id"');
    expect(log).toContain('"url":"/invite/:code"');
    expect(log).toContain('"url":"/media/:file"');
    expect(log).not.toMatch(/SecretShareId42|SecretInviteCode7|SecretPath/);
  });
});
