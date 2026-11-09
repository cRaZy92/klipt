import { afterEach, describe, expect, it } from 'vitest';
import type { FastifyInstance } from 'fastify';
import { buildApp } from '../src/app.js';
import { loadConfig } from '../src/config.js';

describe('GET /api/health', () => {
  let app: FastifyInstance;
  afterEach(async () => {
    await app.close();
  });

  it('returns ok', async () => {
    const config = loadConfig({ BASE_URL: 'http://localhost:3000', DATABASE_URL: 'postgres://x' });
    app = await buildApp({ config, db: {} });
    const res = await app.inject({ method: 'GET', url: '/api/health' });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toEqual({ ok: true });
  });
});
