import { eq } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { ACTIVITY_COUNTERS, ACTIVITY_FIRSTS, type ActivityReport } from '@klipt/shared';
import { requireUser } from '../auth/plugin.js';
import { users } from '../db/schema.js';
import { evaluateUser, markSeen, myAchievements } from './engine.js';
import { mergeActivity, readActivity } from './stats.js';

const MAX_COUNTER = 1e13;
const reportBody = z.object({
  counters: z.record(z.string().max(40), z.unknown()),
  firsts: z.array(z.string().max(40)).max(100),
  backfill: z.boolean().optional(),
});
const seenBody = z.object({ ids: z.array(z.string().max(64)).max(200).optional() });

/** A report as stored; null when a known counter isn't a whole number in range. Unknown keys (a newer app) are dropped. */
function cleanReport(body: z.infer<typeof reportBody>): ActivityReport | null {
  const counters: ActivityReport['counters'] = {};
  for (const key of ACTIVITY_COUNTERS) {
    const v = body.counters[key];
    if (v === undefined) continue;
    if (typeof v !== 'number' || !Number.isInteger(v) || v < 0 || v > MAX_COUNTER) return null;
    counters[key] = v;
  }
  const firsts = ACTIVITY_FIRSTS.filter((f) => body.firsts.includes(f));
  return { counters, firsts, backfill: body.backfill };
}

export async function achievementRoutes(app: FastifyInstance): Promise<void> {
  const { db } = app;

  app.get('/api/me/achievements', { preHandler: requireUser }, async (req) => myAchievements(db, req.user!.id));

  app.post('/api/me/achievements/seen', { preHandler: requireUser }, async (req, reply) => {
    const parsed = seenBody.safeParse(req.body ?? {});
    if (!parsed.success) return reply.code(400).send({ error: 'invalid_request' });
    await markSeen(db, req.user!.id, parsed.data.ids);
    return reply.code(204).send();
  });

  // What only the desktop app knows (clips saved, hours recorded, editor firsts…).
  app.post('/api/me/activity', { preHandler: requireUser }, async (req, reply) => {
    const parsed = reportBody.safeParse(req.body);
    const report = parsed.success ? cleanReport(parsed.data) : null;
    if (!report) return reply.code(400).send({ error: 'invalid_request' });
    const userId = req.user!.id;
    await db.transaction(async (tx) => {
      const [row] = await tx.select({ activity: users.activity }).from(users).where(eq(users.id, userId)).for('update');
      await tx.update(users).set({ activity: mergeActivity(readActivity(row?.activity), report) }).where(eq(users.id, userId));
    });
    await evaluateUser(db, userId, { quiet: report.backfill === true });
    return myAchievements(db, userId);
  });
}
