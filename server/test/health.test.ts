import { afterEach, describe, expect, it } from 'vitest';
import { makeTestApp, type TestApp } from './helpers.js';

describe('GET /api/health', () => {
  let t: TestApp;
  afterEach(async () => {
    await t.close();
  });

  it('returns ok', async () => {
    t = await makeTestApp();
    const res = await t.app.inject({ method: 'GET', url: '/api/health' });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toEqual({ ok: true });
  });
});
