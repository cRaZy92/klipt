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
  });

  it('uses insecure cookies for http BASE_URL and reads overrides', () => {
    const c = loadConfig({
      BASE_URL: 'http://localhost:3000',
      DATABASE_URL: 'postgres://x',
      PORT: '8080',
      MAX_UPLOAD_MB: '10',
      DATA_DIR: '/tmp/d',
      ADMIN_USERNAME: 'root',
      ADMIN_PASSWORD: 'hunter2hunter2',
    });
    expect(c.secureCookies).toBe(false);
    expect(c.port).toBe(8080);
    expect(c.maxUploadBytes).toBe(10 * 1024 * 1024);
    expect(c.dataDir).toBe('/tmp/d');
    expect(c.adminUsername).toBe('root');
    expect(c.adminPassword).toBe('hunter2hunter2');
  });

  it('throws naming BASE_URL when missing', () => {
    expect(() => loadConfig({})).toThrow(/BASE_URL/);
  });
});
