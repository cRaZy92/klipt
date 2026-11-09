import Fastify, { type FastifyInstance } from 'fastify';
import type { HealthResponse } from '@klipt/shared';
import type { Config } from './config.js';
import type { Db } from './db/client.js';

export interface AppDeps {
  config: Config;
  db: Db;
}

export async function buildApp(deps: AppDeps): Promise<FastifyInstance> {
  const app = Fastify({ trustProxy: true });
  app.decorate('config', deps.config);

  app.get('/api/health', async (): Promise<HealthResponse> => ({ ok: true }));

  return app;
}

declare module 'fastify' {
  interface FastifyInstance {
    config: Config;
  }
}
