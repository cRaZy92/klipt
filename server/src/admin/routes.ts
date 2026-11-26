import { and, eq, isNull } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import { nanoid } from 'nanoid';
import { z } from 'zod';
import type { AdminStats, AdminTranscodes, AdminUser, FeedbackItem, Invite, PasswordReset } from '@klipt/shared';
import { clips, feedback, invites, passwordResets, users } from '../db/schema.js';
import { getAdminStats, getTranscodes, listActiveInvites, listAdminUsers, listFeedback, toInvite } from './queries.js';
import { requireAdmin } from '../auth/plugin.js';
import { revokeAllForUser } from '../auth/sessions.js';

const INVITE_TTL_MS = 7 * 24 * 60 * 60 * 1000;
const RESET_TTL_MS = 24 * 60 * 60 * 1000;
const resolveBody = z.object({ resolved: z.boolean() });
const patchBody = z.object({ disabled: z.boolean() });

export async function adminRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;
  const opts = { preHandler: requireAdmin };
  app.post('/api/admin/invites', opts, async (req) => {
    const [row] = await db
      .insert(invites)
      .values({ code: nanoid(16), createdBy: req.user!.id, expiresAt: new Date(Date.now() + INVITE_TTL_MS) })
      .returning();
    return toInvite(row!, config.baseUrl);
  });

  app.get('/api/admin/invites', opts, async (): Promise<Invite[]> => listActiveInvites(db, config.baseUrl));

  app.delete('/api/admin/invites/:code', opts, async (req, reply) => {
    const { code } = req.params as { code: string };
    const res = await db
      .update(invites)
      .set({ revokedAt: new Date() })
      .where(and(eq(invites.code, code), isNull(invites.usedBy), isNull(invites.revokedAt)))
      .returning({ code: invites.code });
    if (res.length === 0) return reply.code(404).send({ error: 'not_found' });
    return reply.code(204).send();
  });

  app.get('/api/admin/users', opts, async (): Promise<AdminUser[]> => listAdminUsers(db, app.config.baseUrl));

  app.patch('/api/admin/users/:id', opts, async (req, reply) => {
    const { id } = req.params as { id: string };
    const parsed = patchBody.safeParse(req.body);
    if (!parsed.success || !z.uuid().safeParse(id).success) return reply.code(400).send({ error: 'invalid_request' });
    if (id === req.user!.id) return reply.code(400).send({ error: 'cannot_disable_self' });
    const found = await db.transaction(async (tx) => {
      const updated = await tx
        .update(users)
        .set({ disabledAt: parsed.data.disabled ? new Date() : null })
        .where(eq(users.id, id))
        .returning({ id: users.id });
      if (updated.length === 0) return false;
      if (parsed.data.disabled) await revokeAllForUser(tx, id);
      return true;
    });
    if (!found) return reply.code(404).send({ error: 'not_found' });
    return reply.code(204).send();
  });

  // A one-time link for a user who forgot their password (valid a day; a new one replaces it).
  app.post('/api/admin/users/:id/password-reset', opts, async (req, reply): Promise<PasswordReset | undefined> => {
    const { id } = req.params as { id: string };
    if (!z.uuid().safeParse(id).success) return reply.code(404).send({ error: 'not_found' });
    const [user] = await db.select({ id: users.id }).from(users).where(eq(users.id, id));
    if (!user) return reply.code(404).send({ error: 'not_found' });
    const code = nanoid(24);
    const expiresAt = new Date(Date.now() + RESET_TTL_MS);
    await db.transaction(async (tx) => {
      await tx.update(passwordResets).set({ usedAt: new Date() }).where(and(eq(passwordResets.userId, id), isNull(passwordResets.usedAt)));
      await tx.insert(passwordResets).values({ code, userId: id, createdBy: req.user!.id, expiresAt });
    });
    return { url: `${config.baseUrl}/reset/${code}`, expiresAt: expiresAt.toISOString() };
  });

  app.get('/api/admin/feedback', opts, async (): Promise<FeedbackItem[]> => listFeedback(db));

  // The log a report came with, as plain text (opened in a new tab or saved).
  app.get('/api/admin/feedback/:id/logs', opts, async (req, reply) => {
    const { id } = req.params as { id: string };
    if (!z.uuid().safeParse(id).success) return reply.code(404).send({ error: 'not_found' });
    const [row] = await db.select({ logs: feedback.logs, createdAt: feedback.createdAt }).from(feedback).where(eq(feedback.id, id));
    if (!row?.logs) return reply.code(404).send({ error: 'not_found' });
    const name = `klipt-log-${row.createdAt.toISOString().slice(0, 10)}-${id.slice(0, 8)}.txt`;
    return reply
      .header('content-type', 'text/plain; charset=utf-8')
      .header('content-disposition', `inline; filename="${name}"`)
      .header('cache-control', 'private, no-store')
      .header('x-content-type-options', 'nosniff')
      .send(row.logs);
  });

  app.patch('/api/admin/feedback/:id', opts, async (req, reply) => {
    const { id } = req.params as { id: string };
    const parsed = resolveBody.safeParse(req.body);
    if (!parsed.success || !z.uuid().safeParse(id).success) return reply.code(400).send({ error: 'invalid_request' });
    const res = await db
      .update(feedback)
      .set({ resolvedAt: parsed.data.resolved ? new Date() : null })
      .where(eq(feedback.id, id))
      .returning({ id: feedback.id });
    if (res.length === 0) return reply.code(404).send({ error: 'not_found' });
    return reply.code(204).send();
  });

  app.delete('/api/admin/feedback/:id', opts, async (req, reply) => {
    const { id } = req.params as { id: string };
    if (!z.uuid().safeParse(id).success) return reply.code(404).send({ error: 'not_found' });
    const res = await db.delete(feedback).where(eq(feedback.id, id)).returning({ id: feedback.id });
    if (res.length === 0) return reply.code(404).send({ error: 'not_found' });
    return reply.code(204).send();
  });

  app.get('/api/admin/stats', opts, async (): Promise<AdminStats> => getAdminStats(db, app.disk));

  app.get('/api/admin/transcodes', opts, async (): Promise<AdminTranscodes> => getTranscodes(db, app.transcoder));

  // A failed compression goes back in the queue.
  app.post('/api/admin/transcodes/:id/retry', opts, async (req, reply) => {
    const { id } = req.params as { id: string };
    const res = await db
      .update(clips)
      .set({ transcodeState: 'queued', transcodeError: null, transcodeStartedAt: null, transcodeFinishedAt: null })
      .where(and(eq(clips.id, id), eq(clips.transcodeState, 'failed')))
      .returning({ id: clips.id });
    if (res.length === 0) return reply.code(404).send({ error: 'not_found' });
    app.transcoder.kick();
    return reply.code(204).send();
  });
}
