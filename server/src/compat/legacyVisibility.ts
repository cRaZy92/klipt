import type { FastifyInstance, FastifyRequest } from 'fastify';

/**
 * Desktop apps from before Friends/Public only know the visibilities 'feed' and 'link' (older
 * Rust code rejects anything else). The new app says `x-klipt-api: 2`; a desktop request (a Bearer
 * token) without it gets 'feed' for Friends and Public clips in every JSON answer.
 */
export function isLegacyApp(req: FastifyRequest): boolean {
  const auth = req.headers.authorization ?? '';
  return auth.startsWith('Bearer ') && !req.headers['x-klipt-api'];
}

/** `value` with every `visibility: 'friends' | 'public'` turned into 'feed' (a copy where changed). */
export function legacyVisibility(value: unknown, depth = 0): unknown {
  if (depth > 6 || value === null || typeof value !== 'object') return value;
  if (Array.isArray(value)) return value.map((v) => legacyVisibility(v, depth + 1));
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(value)) {
    out[k] = k === 'visibility' && (v === 'friends' || v === 'public') ? 'feed' : legacyVisibility(v, depth + 1);
  }
  return out;
}

export function registerLegacyVisibility(app: FastifyInstance): void {
  app.addHook('preSerialization', async (req, _reply, payload) => (isLegacyApp(req) ? legacyVisibility(payload) : payload));
}
