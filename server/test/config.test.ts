import { describe, expect, it } from 'vitest';
import { loadConfig } from '../src/config.js';

describe('loadConfig', () => {
  it('applies defaults and strips trailing slash from BASE_URL', () => {
    const c = loadConfig({ BASE_URL: 'https://clips.example.com/', DATABASE_URL: 'postgres://x' });
    expect(c.baseUrl).toBe('https://clips.example.com');
    expect(c.port).toBe(3000);
    expect(c.dataDir).toBe('/data');
    expect(c.maxUploadBytes).toBe(2048 * 1024 * 1024);
    expect(c.secureCookies).toBe(true);
    expect(c.trustProxyHops).toBe(1);
    expect(c.transcode).toEqual({ enabled: true, crf: 23, preset: 'medium', threads: 2 });
  });

  it('reads the compression settings and refuses bad ones', () => {
    const base = { BASE_URL: 'http://localhost', DATABASE_URL: 'postgres://x' };
    const c = loadConfig({ ...base, TRANSCODE: 'off', TRANSCODE_CRF: '26', TRANSCODE_PRESET: 'fast', TRANSCODE_THREADS: '1' });
    expect(c.transcode).toEqual({ enabled: false, crf: 26, preset: 'fast', threads: 1 });
    expect(() => loadConfig({ ...base, TRANSCODE_THREADS: '0' })).toThrow(/TRANSCODE_THREADS/);
    expect(() => loadConfig({ ...base, TRANSCODE_PRESET: 'turbo' })).toThrow(/TRANSCODE_PRESET/);
    expect(() => loadConfig({ ...base, TRANSCODE: 'yes' })).toThrow(/TRANSCODE/);
  });

  it('uses insecure cookies for http BASE_URL and reads overrides', () => {
    const c = loadConfig({
      BASE_URL: 'http://localhost:3000',
      DATABASE_URL: 'postgres://x',
      PORT: '8080',
      TRUST_PROXY_HOPS: '2',
      MAX_UPLOAD_MB: '10',
      DATA_DIR: '/tmp/d',
      ADMIN_USERNAME: 'root',
      ADMIN_PASSWORD: 'hunter2hunter2',
    });
    expect(c.secureCookies).toBe(false);
    expect(c.port).toBe(8080);
    expect(c.trustProxyHops).toBe(2);
    expect(c.maxUploadBytes).toBe(10 * 1024 * 1024);
    expect(c.dataDir).toBe('/tmp/d');
    expect(c.adminUsername).toBe('root');
    expect(c.adminPassword).toBe('hunter2hunter2');
  });

  it('throws naming BASE_URL when missing', () => {
    expect(() => loadConfig({})).toThrow(/BASE_URL/);
  });
});
