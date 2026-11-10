import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
beforeEach(async () => {
  t = await makeTestApp();
  t.app.get('/api/test-boom', async () => {
    throw new Error('secret db detail');
  });
  t.app.get('/test-boom', async () => {
    throw new Error('secret db detail');
  });
});
afterEach(() => t.close());

describe('error handler', () => {
  it('answers API errors with a generic internal_error', async () => {
    const res = await t.app.inject({ method: 'GET', url: '/api/test-boom' });
    expect(res.statusCode).toBe(500);
    expect(res.json()).toEqual({ error: 'internal_error' });
    expect(res.body).not.toContain('secret');
  });

  it('renders an HTML 500 page for page routes', async () => {
    const res = await t.app.inject({ method: 'GET', url: '/test-boom' });
    expect(res.statusCode).toBe(500);
    expect(res.headers['content-type']).toContain('text/html');
    expect(res.body).toContain('Something went wrong');
    expect(res.body).not.toContain('secret');
  });

  it('keeps 4xx responses unchanged (validation, bad JSON, rate limit)', async () => {
    const u = await createUser(t.db, { username: 'alice' });
    const { headers } = await sessionFor(t.db, u.id);
    const invalid = await t.app.inject({ method: 'POST', url: '/api/clips', headers, payload: { title: '' } });
    expect(invalid.statusCode).toBe(400);
    expect(invalid.json()).toEqual({ error: 'invalid_request' });

    const badJson = await t.app.inject({
      method: 'POST',
      url: '/api/auth/login',
      headers: { 'content-type': 'application/json' },
      payload: '{nope',
    });
    expect(badJson.statusCode).toBe(400);
    expect(badJson.json()).toMatchObject({ statusCode: 400, code: 'FST_ERR_CTP_INVALID_JSON_BODY', error: 'Bad Request' });

    let last;
    for (let i = 0; i < 11; i++) {
      last = await t.app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'alice', password: 'wrong', client: 'web' } });
    }
    expect(last!.statusCode).toBe(429);
    expect(last!.json()).toMatchObject({ statusCode: 429, error: 'Too Many Requests' });
  });
});
