import Fastify, { type FastifyError, type FastifyInstance, type FastifyRequest } from 'fastify';
import type { HealthResponse } from '@klipt/shared';
import type { Config } from './config.js';
import type { Db } from './db/client.js';
import { registerAuth } from './auth/plugin.js';
import { authRoutes } from './auth/routes.js';
import { adminRoutes } from './admin/routes.js';
import { mountTus } from './uploads/tus.js';
import { clipRoutes } from './clips/routes.js';
import { webRoutes } from './web/routes.js';
import { join } from 'node:path';
import { Transcoder } from './clips/transcoder.js';
import { ServerErrorPage } from './web/pages/serverError.js';

export interface AppDeps {
  config: Config;
  db: Db;
  /** Enables Fastify's pino logger (on in production, off in tests). */
  logger?: boolean;
  /** Where log lines go when `logger` is on (stdout by default; tests capture them). */
  logStream?: { write(line: string): void };
}

/**
 * The URL as logged: the matched route pattern (`/c/:id`), never the real path. Paths carry
 * link-only share ids and invite codes, and anyone with server logs could open those.
 */
export function loggedUrl(req: Pick<FastifyRequest, 'routeOptions'>): string {
  return req.routeOptions?.url ?? '(no route)';
}

export async function buildApp(deps: AppDeps): Promise<FastifyInstance> {
  const app = Fastify({
    logger: deps.logger
      ? {
          ...(deps.logStream ? { stream: deps.logStream } : {}),
          serializers: {
            req: (req: FastifyRequest) => ({ method: req.method, url: loggedUrl(req), remoteAddress: req.ip }),
          },
        }
      : false,
    trustProxy: (_addr: string, hop: number) => hop < deps.config.trustProxyHops,
  });
  app.decorate('config', deps.config);
  app.decorate('db', deps.db);
  const transcoder = new Transcoder({
    db: deps.db,
    clipsDir: join(deps.config.dataDir, 'clips'),
    log: app.log,
    ...deps.config.transcode,
  });
  app.decorate('transcoder', transcoder);
  app.addHook('onReady', () => transcoder.start());
  app.addHook('onClose', () => transcoder.stop());

  // Unexpected failures are logged and answered generically, never with err.message.
  // 4xx errors (bad JSON, body too large, rate limit, ...) go to Fastify's default handler unchanged.
  app.setErrorHandler((err: FastifyError, req, reply) => {
    if ((err.statusCode ?? 500) < 500) return reply.send(err);
    req.log.error({ err }, 'request failed');
    reply.code(500);
    if (req.url.startsWith('/api/') || req.url.startsWith('/media/')) return reply.send({ error: 'internal_error' });
    return reply
      .header('content-type', 'text/html; charset=utf-8')
      .header('cache-control', 'no-store')
      .send(ServerErrorPage({ user: req.user ?? null }) as string);
  });
  await registerAuth(app);

  app.get('/api/health', async (): Promise<HealthResponse> => ({ ok: true }));

  await authRoutes(app);
  await adminRoutes(app);
  await mountTus(app, { db: deps.db, config: deps.config });
  await clipRoutes(app);
  await webRoutes(app);

  return app;
}

declare module 'fastify' {
  interface FastifyInstance {
    config: Config;
    db: Db;
    transcoder: Transcoder;
  }
}
