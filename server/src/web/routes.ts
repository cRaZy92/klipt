import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import fastifyStatic from '@fastify/static';
import { and, eq, gt, isNull, sql } from 'drizzle-orm';
import type { FastifyInstance, FastifyReply, FastifyRequest } from 'fastify';
import type { AdminUser, Invite, SessionInfo } from '@klipt/shared';
import { clips, invites, sessions, users } from '../db/schema.js';
import { listClips, listGames, InvalidCursorError, type Scope } from '../clips/repo.js';
import { AdminPage } from './pages/admin.js';
import { FeedPage } from './pages/feed.js';
import { InvitePage } from './pages/invite.js';
import { LoginPage } from './pages/login.js';
import { MePage } from './pages/me.js';
import { NotFoundPage } from './pages/notFound.js';
import { UserPage } from './pages/user.js';

/** Only same-site absolute paths: one leading slash, no `//`, no backslashes, no control chars. */
export function safeNext(next: unknown): string {
  if (typeof next !== 'string') return '/';
  if (!/^\/(?![/\\])[^\\\x00-\x1f]*$/.test(next)) return '/';
  return next;
}

function publicDir(): string {
  const here = dirname(fileURLToPath(import.meta.url));
  const candidates = [process.env.PUBLIC_DIR, join(here, '../../public'), join(here, '../public'), join(process.cwd(), 'public')];
  return candidates.find((c) => c && existsSync(join(c, 'app.css'))) ?? join(here, '../../public');
}

export async function webRoutes(app: FastifyInstance): Promise<void> {
  const { db, config } = app;

  await app.register(fastifyStatic, {
    root: publicDir(),
    prefix: '/static/',
    decorateReply: false,
    cacheControl: true,
    maxAge: '5m',
  });

  const html = (reply: FastifyReply, body: string, code = 200) =>
    reply.code(code).header('content-type', 'text/html; charset=utf-8').header('cache-control', 'private, no-cache').send(body);

  const notFound = (req: FastifyRequest, reply: FastifyReply) => html(reply, NotFoundPage({ user: req.user }) as string, 404);

  /** Redirects to login when logged out; returns false in that case. */
  const guard = (req: FastifyRequest, reply: FastifyReply): boolean => {
    if (req.user) return true;
    void reply.redirect(`/login?next=${encodeURIComponent(req.url)}`);
    return false;
  };

  const listQuery = (req: FastifyRequest) => {
    const q = req.query as { game?: unknown; cursor?: unknown };
    return {
      game: typeof q.game === 'string' && q.game ? q.game.slice(0, 100) : undefined,
      cursor: typeof q.cursor === 'string' && q.cursor ? q.cursor.slice(0, 512) : undefined,
    };
  };

  /** Shared list loading; an invalid cursor falls back to the first page. */
  async function load(req: FastifyRequest, scope: Scope, username?: string) {
    const { game, cursor } = listQuery(req);
    const user = req.user!;
    const run = (c?: string) => listClips(db, user, { scope, username, game, cursor: c });
    const [page, games] = await Promise.all([
      run(cursor).catch((err) => (err instanceof InvalidCursorError ? run() : Promise.reject(err))),
      listGames(db, user, scope, username),
    ]);
    return { ...page, games, game };
  }

  app.get('/login', async (req, reply) => {
    const next = safeNext((req.query as { next?: unknown }).next);
    if (req.user) return reply.redirect(next);
    return html(reply, LoginPage({ next }) as string);
  });

  app.get('/invite/:code', async (req, reply) => {
    const { code } = req.params as { code: string };
    const [row] = await db
      .select({ code: invites.code })
      .from(invites)
      .where(and(eq(invites.code, code.slice(0, 100)), isNull(invites.usedBy), isNull(invites.revokedAt), gt(invites.expiresAt, new Date())));
    return html(reply, InvitePage({ code, valid: !!row }) as string);
  });

  app.get('/', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    const data = await load(req, 'feed');
    return html(reply, FeedPage({ user: req.user!, ...data }) as string);
  });

  app.get('/u/:username', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    const { username } = req.params as { username: string };
    const [owner] = await db
      .select({ username: users.username })
      .from(users)
      .where(eq(users.usernameKey, username.toLowerCase()));
    if (!owner) return notFound(req, reply);
    const data = await load(req, 'user', owner.username);
    return html(reply, UserPage({ user: req.user!, username: owner.username, ...data }) as string);
  });

  app.get('/me', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    const user = req.user!;
    const data = await load(req, 'mine');
    const rows = await db.select().from(sessions).where(eq(sessions.userId, user.id)).orderBy(sessions.createdAt);
    const sessionInfos: SessionInfo[] = rows.map((s) => ({
      id: s.id,
      kind: s.kind,
      label: s.label,
      createdAt: s.createdAt.toISOString(),
      lastUsedAt: s.lastUsedAt.toISOString(),
      current: s.id === user.sessionId,
    }));
    return html(reply, MePage({ user, ...data, sessions: sessionInfos, baseUrl: config.baseUrl }) as string);
  });

  app.get('/admin', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    if (!req.user!.isAdmin) return notFound(req, reply);
    const [u] = await db.select({ n: sql<number>`count(*)::int` }).from(users);
    const [c] = await db
      .select({ n: sql<number>`count(*)::int`, bytes: sql<number>`coalesce(sum(${clips.sizeBytes}), 0)::float8` })
      .from(clips);
    const inviteRows = await db
      .select()
      .from(invites)
      .where(and(isNull(invites.usedBy), isNull(invites.revokedAt), gt(invites.expiresAt, new Date())))
      .orderBy(invites.createdAt);
    const userRows = await db
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
    const inviteList: Invite[] = inviteRows.map((i) => ({
      code: i.code,
      url: `${config.baseUrl}/invite/${i.code}`,
      expiresAt: i.expiresAt.toISOString(),
    }));
    const userList: AdminUser[] = userRows.map((r) => ({
      id: r.id,
      username: r.username,
      isAdmin: r.isAdmin,
      disabled: r.disabledAt !== null,
      clipCount: r.clipCount,
      createdAt: r.createdAt.toISOString(),
    }));
    return html(
      reply,
      AdminPage({ user: req.user!, stats: { users: u!.n, clips: c!.n, storageBytes: Number(c!.bytes) }, invites: inviteList, users: userList }) as string,
    );
  });

  app.setNotFoundHandler((req, reply) => {
    if (req.url.startsWith('/api/') || req.url.startsWith('/media/')) return reply.code(404).send({ error: 'not_found' });
    return notFound(req, reply);
  });
}
