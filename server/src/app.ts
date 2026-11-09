import Fastify, { type FastifyInstance } from 'fastify';
import type { HealthResponse } from '@klipt/shared';
import type { Config } from './config.js';
import type { Db } from './db/client.js';
import { registerAuth } from './auth/plugin.js';
import { authRoutes } from './auth/routes.js';

export interface AppDeps {
  config: Config;
  db: Db;
}

export async function buildApp(deps: AppDeps): Promise<FastifyInstance> {
  const app = Fastify({ trustProxy: true });
  app.decorate('config', deps.config);
  app.decorate('db', deps.db);
  await registerAuth(app);

  app.get('/api/health', async (): Promise<HealthResponse> => ({ ok: true }));

  await authRoutes(app);

  return app;
}

declare module 'fastify' {
  interface FastifyInstance {
    config: Config;
    db: Db;
  }
}
