import cookie from '@fastify/cookie';
import rateLimit from '@fastify/rate-limit';
import type { FastifyInstance, FastifyReply, FastifyRequest } from 'fastify';
import { resolveToken, type AuthUser } from './sessions.js';

export const SESSION_COOKIE = 'klipt_session';

/** Per-route rate limit config: `{ config: { rateLimit: authRateLimit } }` (login, register). */
export const authRateLimit = { max: 10, timeWindow: '1 minute' } as const;

declare module 'fastify' {
  interface FastifyRequest {
    user: AuthUser | null;
  }
}

/** Registered directly on the root instance (not encapsulated) so decorations are visible everywhere. */
export async function registerAuth(app: FastifyInstance): Promise<void> {
  await app.register(cookie);
  // global: false, so only routes that opt in via config.rateLimit are limited.
  await app.register(rateLimit, { global: false });

  app.decorateRequest('user', null);
  app.addHook('onRequest', async (req) => {
    const header = req.headers.authorization;
    let token: string | undefined;
    if (header && /^bearer /i.test(header)) token = header.slice(7).trim();
    else token = req.cookies[SESSION_COOKIE];
    req.user = token ? await resolveToken(app.db, token) : null;
  });
}

export async function requireUser(req: FastifyRequest, reply: FastifyReply): Promise<void> {
  if (!req.user) return reply.code(401).send({ error: 'unauthorized' });
}

export async function requireAdmin(req: FastifyRequest, reply: FastifyReply): Promise<void> {
  if (!req.user) return reply.code(401).send({ error: 'unauthorized' });
  if (!req.user.isAdmin) return reply.code(403).send({ error: 'forbidden' });
}
