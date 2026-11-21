import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { requireUser } from '../auth/plugin.js';
import { feedback } from '../db/schema.js';

/** The most of the app's log a report keeps (its end). */
export const MAX_LOG_CHARS = 2_000_000;

const body = z.object({
  kind: z.enum(['bug', 'idea']),
  message: z.string().trim().min(1).max(10_000),
  source: z.enum(['desktop', 'web']),
  system: z.record(z.string().max(100), z.string().max(2000)).optional(),
  logs: z.string().optional(),
});

/** Problem reports and feature ideas from the app or the website; admins read them on the admin page. */
export async function feedbackRoutes(app: FastifyInstance): Promise<void> {
  const { db } = app;
  app.post(
    '/api/feedback',
    {
      preHandler: requireUser,
      // Logs make a report bigger than an ordinary JSON body.
      bodyLimit: 4 * 1024 * 1024,
      config: { rateLimit: { max: 10, timeWindow: '10 minutes' } },
    },
    async (req, reply) => {
      const parsed = body.safeParse(req.body);
      if (!parsed.success) return reply.code(400).send({ error: 'invalid_request' });
      const { kind, message, source, system, logs } = parsed.data;
      const systemEntries = system ? Object.entries(system).slice(0, 60) : [];
      const [row] = await db
        .insert(feedback)
        .values({
          userId: req.user!.id,
          kind,
          message,
          source,
          system: systemEntries.length ? Object.fromEntries(systemEntries) : null,
          logs: logs ? logs.slice(-MAX_LOG_CHARS) : null,
        })
        .returning({ id: feedback.id });
      req.log.info({ feedbackId: row!.id, kind }, 'feedback received');
      return reply.code(201).send({ id: row!.id });
    },
  );
}
