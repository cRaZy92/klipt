import { and, eq, isNull } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import type { FriendList, FriendState } from '@klipt/shared';
import { requireUser } from '../auth/plugin.js';
import { users } from '../db/schema.js';
import { toUserRef } from '../users/ref.js';
import { listFriendRows, removeFriend, requestFriend } from './repo.js';

const USERNAME = /^[A-Za-z0-9_]{1,64}$/;

export async function friendRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;

  /** The active user called `name` (any case), or null. */
  async function userNamed(name: string): Promise<string | null> {
    if (!USERNAME.test(name)) return null;
    const [row] = await db
      .select({ id: users.id })
      .from(users)
      .where(and(eq(users.usernameKey, name.toLowerCase()), isNull(users.disabledAt)));
    return row?.id ?? null;
  }

  app.get('/api/friends', { preHandler: requireUser }, async (req): Promise<FriendList> => {
    const rows = await listFriendRows(db, req.user!.id);
    const refs = (list: typeof rows.friends) => list.map((r) => toUserRef(r, config.baseUrl));
    return { friends: refs(rows.friends), incoming: refs(rows.incoming), outgoing: refs(rows.outgoing) };
  });

  app.post('/api/friends/:username', { preHandler: requireUser }, async (req, reply) => {
    const { username } = req.params as { username: string };
    const other = await userNamed(username);
    if (!other) return reply.code(404).send({ error: 'unknown_user', message: `No user called ${username}.` });
    if (other === req.user!.id) return reply.code(400).send({ error: 'self', message: "You can't add yourself." });
    const state: FriendState = await requestFriend(db, req.user!.id, other);
    return { state };
  });

  app.delete('/api/friends/:username', { preHandler: requireUser }, async (req, reply) => {
    const { username } = req.params as { username: string };
    const other = await userNamed(username);
    if (other && other !== req.user!.id) await removeFriend(db, req.user!.id, other);
    return reply.code(204).send();
  });
}
