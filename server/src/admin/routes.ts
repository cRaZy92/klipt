import { and, eq, gt, isNull, sql } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import { nanoid } from 'nanoid';
import { z } from 'zod';
import type { AdminStats, AdminUser, Invite } from '@klipt/shared';
import { clips, invites, users } from '../db/schema.js';
import { requireAdmin } from '../auth/plugin.js';
import { revokeAllForUser } from '../auth/sessions.js';

const INVITE_TTL_MS = 7 * 24 * 60 * 60 * 1000;
const patchBody = z.object({ disabled: z.boolean() });

export async function adminRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;
  const opts = { preHandler: requireAdmin };
  const toInvite = (i: { code: string; expiresAt: Date }): Invite => ({
    code: i.code,
    url: `${config.baseUrl}/invite/${i.code}`,
    expiresAt: i.expiresAt.toISOString(),
  });

  app.post('/api/admin/invites', opts, async (req) => {
    const [row] = await db
      .insert(invites)
      .values({ code: nanoid(16), createdBy: req.user!.id, expiresAt: new Date(Date.now() + INVITE_TTL_MS) })
      .returning();
    return toInvite(row!);
  });

  app.get('/api/admin/invites', opts, async (): Promise<Invite[]> => {
    const rows = await db
      .select()
      .from(invites)
      .where(and(isNull(invites.usedBy), isNull(invites.revokedAt), gt(invites.expiresAt, new Date())))
      .orderBy(invites.createdAt);
    return rows.map(toInvite);
  });

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

  app.get('/api/admin/users', opts, async (): Promise<AdminUser[]> => {
    const rows = await db
      .select({
        id: users.id,
        username: users.username,
        isAdmin: users.isAdmin,
        disabledAt: users.disabledAt,
        createdAt: users.createdAt,
        clipCount: sql<number>`count(${clips.id})::int`,
      })
      .from(users)
      .leftJoin(clips, eq(clips.userId, users.id))
      .groupBy(users.id)
      .orderBy(users.createdAt);
    return rows.map((u) => ({
      id: u.id,
      username: u.username,
      isAdmin: u.isAdmin,
      disabled: u.disabledAt !== null,
      clipCount: u.clipCount,
      createdAt: u.createdAt.toISOString(),
    }));
  });

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

  app.get('/api/admin/stats', opts, async (): Promise<AdminStats> => {
    const [u] = await db.select({ n: sql<number>`count(*)::int` }).from(users);
    const [c] = await db
      .select({ n: sql<number>`count(*)::int`, bytes: sql<number>`coalesce(sum(${clips.sizeBytes}), 0)::float8` })
      .from(clips);
    return { users: u!.n, clips: c!.n, storageBytes: Number(c!.bytes) };
  });
}
