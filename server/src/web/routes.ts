import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import fastifyStatic from '@fastify/static';
import { eq } from 'drizzle-orm';
import type { FastifyInstance, FastifyReply, FastifyRequest } from 'fastify';
import { getAdminStats, isInviteActive, listActiveInvites, listAdminUsers } from '../admin/queries.js';
import { toClip } from '../clips/routes.js';
import { listSessions } from '../auth/sessions.js';
import { users } from '../db/schema.js';
import { getClip, getUserStorage, listClips, listGames, listViewers, InvalidCursorError, type Scope } from '../clips/repo.js';
import { AdminPage } from './pages/admin.js';
import { FeedPage } from './pages/feed.js';
import { InvitePage } from './pages/invite.js';
import { LoginPage } from './pages/login.js';
import { SharePage } from './pages/share.js';
import { MePage } from './pages/me.js';
import { NotFoundPage } from './pages/notFound.js';
import { UserPage } from './pages/user.js';
import { createSteamArt, parseAppId } from './steamArt.js';

/**
 * Only same-site absolute paths: one leading slash, no `//`, no backslashes, and printable ASCII
 * only (a non-ASCII or control character would make the Location header invalid).
 */
export function safeNext(next: unknown): string {
  if (typeof next !== 'string') return '/';
  if (!/^\/(?![/\\])[\x20-\x5b\x5d-\x7e]*$/.test(next)) return '/';
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

  // Game chip art: a redirect to the game's Steam header image. Logged-in only, like the chips,
  // so strangers can't make the server query Steam.
  const steamArt = createSteamArt();
  app.get('/steam-art/:appId', async (req, reply) => {
    const appId = parseAppId((req.params as { appId: string }).appId);
    const url = req.user && appId !== null ? await steamArt(appId) : null;
    if (!url) return reply.code(404).header('cache-control', 'private, max-age=600').send();
    return reply.header('cache-control', 'private, max-age=86400').redirect(url, 302);
  });

  const html =(reply: FastifyReply, body: string, code = 200) =>
    reply.code(code).header('content-type', 'text/html; charset=utf-8').header('cache-control', 'private, no-cache').send(body);

  const notFound = (req: FastifyRequest, reply: FastifyReply) => html(reply, NotFoundPage({ user: req.user }) as string, 404);

  /** Redirects to login when logged out; returns false in that case. */
  const guard = (req: FastifyRequest, reply: FastifyReply): boolean => {
    if (req.user) return true;
    void reply.redirect(`/login?next=${encodeURIComponent(req.url)}`);
    return false;
  };

  const listQuery = (req: FastifyRequest) => {
    const q = req.query as { game?: unknown; q?: unknown; cursor?: unknown };
    return {
      game: typeof q.game === 'string' && q.game ? q.game.slice(0, 100) : undefined,
      search: typeof q.q === 'string' && q.q.trim() ? q.q.trim().slice(0, 100) : undefined,
      cursor: typeof q.cursor === 'string' && q.cursor ? q.cursor.slice(0, 512) : undefined,
    };
  };

  /** Shared list loading; an invalid cursor falls back to the first page. */
  async function load(req: FastifyRequest, scope: Scope, username?: string) {
    const { game, search, cursor } = listQuery(req);
    const user = req.user!;
    const run = (c?: string) => listClips(db, user, { scope, username, game, search, cursor: c });
    const [page, games] = await Promise.all([
      run(cursor).catch((err) => (err instanceof InvalidCursorError ? run() : Promise.reject(err))),
      listGames(db, user, scope, username),
    ]);
    return { ...page, games, game, search };
  }

  app.get('/login', async (req, reply) => {
    const next = safeNext((req.query as { next?: unknown }).next);
    if (req.user) return reply.redirect(next);
    return html(reply, LoginPage({ next }) as string);
  });

  app.get('/invite/:code', async (req, reply) => {
    const { code } = req.params as { code: string };
    const valid = await isInviteActive(db, code.slice(0, 100));
    return html(reply, InvitePage({ code, valid }) as string);
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

  // Public: the id is the secret (link-only clips are shareable too).
  app.get('/c/:id', async (req, reply) => {
    const { id } = req.params as { id: string };
    const found = /^[A-Za-z0-9_-]{12}$/.test(id) ? await getClip(db, id) : null;
    if (!found) return html(reply, NotFoundPage({ user: req.user, message: "This clip doesn't exist or was deleted." }) as string, 404);
    const canDelete = !!req.user && (req.user.id === found.clip.userId || req.user.isAdmin);
    const clip = toClip(found.clip, found.owner, config.baseUrl, found.views);
    const viewers = req.user?.id === found.clip.userId ? (await listViewers(db, clip.id)).map((v) => v.username) : undefined;
    return html(reply, SharePage({ user: req.user, clip, canDelete, viewers }) as string);
  });

  app.get('/me', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    const user = req.user!;
    const data = await load(req, 'mine');
    const [sessionInfos, storage] = await Promise.all([listSessions(db, user), getUserStorage(db, user.id)]);
    return html(reply, MePage({ user, ...data, sessions: sessionInfos, storage, baseUrl: config.baseUrl }) as string);
  });

  app.get('/admin', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    if (!req.user!.isAdmin) return notFound(req, reply);
    const [stats, inviteList, userList] = await Promise.all([getAdminStats(db), listActiveInvites(db, config.baseUrl), listAdminUsers(db)]);
    return html(
      reply,
      AdminPage({ user: req.user!, stats, invites: inviteList, users: userList }) as string,
    );
  });

  app.setNotFoundHandler((req, reply) => {
    if (req.url.startsWith('/api/') || req.url.startsWith('/media/')) return reply.code(404).send({ error: 'not_found' });
    return notFound(req, reply);
  });
}
