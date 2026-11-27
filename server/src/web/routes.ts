import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import fastifyStatic from '@fastify/static';
import { and, eq, isNull, sql } from 'drizzle-orm';
import type { FastifyInstance, FastifyReply, FastifyRequest } from 'fastify';
import { getActivity, getAdminStats, getGameShares, getTranscodes, listAdminUsers, listFeedback } from '../admin/queries.js';
import { toClip, versionQuery } from '../clips/routes.js';
import { listSessions } from '../auth/sessions.js';
import { passwordResets, sessions, users } from '../db/schema.js';
import { activeReset } from '../auth/routes.js';
import { AccountPage } from './pages/account.js';
import { markSeen, myAchievements } from '../achievements/engine.js';
import { toUserRef, userRefColumns } from '../users/ref.js';
import { loadProfile } from '../users/profile.js';
import { listFriendRows } from '../friends/repo.js';
import { FriendsPage } from './pages/friends.js';
import { HomePage } from './pages/home.js';
import { listReferrals, referrerOf, REF_COOKIE } from '../referrals/routes.js';
import { ResetPage } from './pages/reset.js';
import { displayNameOf, getClip, getUserStorage, listClips, listGames, listViewers, InvalidCursorError, type Scope } from '../clips/repo.js';
import { AdminPage, TranscodeSection } from './pages/admin.js';
import { FeedPage } from './pages/feed.js';
import { GetAppPage } from './pages/getApp.js';
import { SignupPage } from './pages/signup.js';
import { LoginPage } from './pages/login.js';
import { SharePage } from './pages/share.js';
import { embedPlan, FULL_VIDEO_EMBED_MAX_BYTES, hasEmbed } from '../clips/embed.js';
import { embedFileName } from '../clips/files.js';
import { MePage } from './pages/me.js';
import { NotFoundPage } from './pages/notFound.js';
import { UserPage } from './pages/user.js';
import { AlbumPage, AlbumsPage } from './pages/albums.js';
import { albumClipList, getAlbum, listAlbums, toAlbumSummary } from '../albums/repo.js';
import { createLatestInstaller } from './latestInstaller.js';
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
    const q = req.query as { game?: unknown; q?: unknown; with?: unknown; kind?: unknown; fav?: unknown; cursor?: unknown };
    return {
      withMe: q.with === 'me',
      favorites: q.fav === '1',
      kind: q.kind === 'clip' || q.kind === 'screenshot' ? (q.kind as 'clip' | 'screenshot') : undefined,
      game: typeof q.game === 'string' && q.game ? q.game.slice(0, 100) : undefined,
      search: typeof q.q === 'string' && q.q.trim() ? q.q.trim().slice(0, 100) : undefined,
      cursor: typeof q.cursor === 'string' && q.cursor ? q.cursor.slice(0, 512) : undefined,
    };
  };

  /** Shared list loading; an invalid cursor falls back to the first page. */
  async function load(req: FastifyRequest, scope: Scope, username?: string) {
    const { game, search, withMe, kind, cursor } = listQuery(req);
    // Favorites is a feed filter only.
    const favorites = scope === 'feed' && listQuery(req).favorites;
    const user = req.user ?? null;
    const run = (c?: string) => listClips(db, user, { scope, username, game, search, withMe, kind, favorites, cursor: c });
    const [page, games] = await Promise.all([
      run(cursor).catch((err) => (err instanceof InvalidCursorError ? run() : Promise.reject(err))),
      listGames(db, user, scope, username),
    ]);
    return { ...page, games, game, search, withMe, kind, favorites };
  }

  app.get('/login', async (req, reply) => {
    const next = safeNext((req.query as { next?: unknown }).next);
    if (req.user) return reply.redirect(next);
    return html(reply, LoginPage({ next }) as string);
  });

  // Anyone can make an account. Coming from a referral link, the page says who invited you.
  app.get('/signup', async (req, reply) => {
    const next = safeNext((req.query as { next?: unknown }).next);
    if (req.user) return reply.redirect(next);
    const referrer = await referrerOf(db, req.cookies[REF_COOKIE]);
    return html(reply, SignupPage({ next, referrer: referrer?.displayName ?? null }) as string);
  });

  // How to install the desktop app. Public, so the link can be sent to a friend; new accounts land
  // here (`welcome=1`), and `next` is where its Continue button goes.
  app.get('/get-app', async (req, reply) => {
    const q = req.query as { next?: unknown; welcome?: unknown };
    return html(reply, GetAppPage({ user: req.user, next: safeNext(q.next), welcome: q.welcome === '1' }) as string);
  });

  // The download button: the newest installer, found through the release's latest.json.
  const latestInstaller = createLatestInstaller();
  app.get('/get-app/windows', async (_req, reply) => {
    return reply.header('cache-control', 'no-store').redirect(await latestInstaller(), 302);
  });

  // Logged out: what Klipt is (and who invited you, from a referral link). Logged in: the feed.
  app.get('/', async (req, reply) => {
    if (!req.user) {
      const referrer = await referrerOf(db, req.cookies[REF_COOKIE]);
      const referralInvalid = (req.query as { ref?: unknown }).ref === 'invalid';
      return html(reply, HomePage({ referrer: referrer?.displayName ?? null, referralInvalid }) as string);
    }
    const data = await load(req, 'feed');
    return html(reply, FeedPage({ user: req.user!, ...data }) as string);
  });

  // Public: anyone sees a Public profile in full; a Friends profile shows strangers only the
  // owner's Public clips and a way to add them.
  app.get('/u/:username', async (req, reply) => {
    const { username } = req.params as { username: string };
    const viewer = req.user ?? null;
    const profile = await loadProfile(db, viewer, username);
    if (!profile) return notFound(req, reply);
    const { owner, stats, banner, friendState: state } = profile;
    const data = await load(req, 'user', owner.username);
    const page = UserPage({
      user: viewer,
      username: owner.username,
      displayName: owner.displayName,
      ...data,
      stats,
      owner,
      banner,
      badges: profile.badges,
      friendState: state,
      loginNext: req.url,
    }) as string;
    // Opening your own profile is seeing your new achievements.
    if (viewer && owner.id === viewer.id && viewer.newAchievements > 0) await markSeen(db, owner.id);
    return html(reply, page);
  });

  // Public: the id is the secret (link-only clips are shareable too).
  app.get('/c/:id', async (req, reply) => {
    const { id } = req.params as { id: string };
    const found = /^[A-Za-z0-9_-]{12}$/.test(id) ? await getClip(db, id, req.user?.id) : null;
    if (!found) return html(reply, NotFoundPage({ user: req.user, message: "This clip doesn't exist or was deleted." }) as string, 404);
    const canDelete = !!req.user && (req.user.id === found.clip.userId || req.user.isAdmin);
    const clip = toClip(found, config.baseUrl);
    const viewers = req.user?.id === found.clip.userId ? (await listViewers(db, clip.id)).map((v) => v.displayName) : undefined;
    const signup = !req.user;
    const plan = clip.kind === 'clip' ? embedPlan(clip) : null;
    // Without a small copy the full video goes in the embed only when Discord can fetch it in time:
    // it gives up on a huge one (an upload still being compressed) and then shows no embed at all.
    const embed =
      plan && (await hasEmbed(join(config.dataDir, 'clips'), clip.id))
        ? { url: `${config.baseUrl}/media/${embedFileName(clip.id)}${versionQuery(found.clip)}`, width: plan.width, height: plan.height }
        : clip.kind === 'clip' && clip.sizeBytes <= FULL_VIDEO_EMBED_MAX_BYTES
          ? { url: clip.videoUrl, width: clip.width, height: clip.height }
          : null;
    return html(reply, SharePage({ user: req.user, clip, embed, canDelete, viewers, signup }) as string);
  });

  app.get('/friends', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    const user = req.user!;
    const [rows, referrals] = await Promise.all([listFriendRows(db, user.id), listReferrals(db, user.id, config.baseUrl)]);
    const refs = (list: typeof rows.friends) => list.map((r) => toUserRef(r, config.baseUrl));
    const ref = (req.query as { ref?: unknown }).ref;
    const notice = ref === 'own' || ref === 'invalid' ? ref : null;
    return html(reply, FriendsPage({ user, friends: refs(rows.friends), incoming: refs(rows.incoming), outgoing: refs(rows.outgoing), referrals, notice }) as string);
  });

  app.get('/albums', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    const albums = (await listAlbums(db, req.user!.id)).map((a) => toAlbumSummary(a, config.baseUrl));
    return html(reply, AlbumsPage({ user: req.user!, albums }) as string);
  });

  // Public, like a clip's share link: the id is the secret. `?c=<clip id>` picks the clip that plays.
  app.get('/a/:id', async (req, reply) => {
    const { id } = req.params as { id: string };
    const found = /^[A-Za-z0-9_-]{12}$/.test(id) ? await getAlbum(db, id) : null;
    if (!found) return html(reply, NotFoundPage({ user: req.user, message: "This album doesn't exist or was deleted." }) as string, 404);
    const clips = (await albumClipList(db, id, req.user?.id)).map((c) => toClip(c, config.baseUrl));
    const pick = (req.query as { c?: unknown }).c;
    const current = clips.find((c) => c.id === pick) ?? clips[0] ?? null;
    const isOwner = req.user?.id === found.album.userId;
    return html(reply, AlbumPage({ user: req.user, album: toAlbumSummary(found, config.baseUrl), clips, current, isOwner }) as string);
  });

  app.get('/me', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    const user = req.user!;
    const data = await load(req, 'mine');
    const storage = await getUserStorage(db, user.id);
    return html(reply, MePage({ user, ...data, storage, baseUrl: config.baseUrl }) as string);
  });

  app.get('/account', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    const user = req.user!;
    const [infos, devices, achievements] = await Promise.all([
      listSessions(db, user),
      db.select({ id: sessions.id, appVersion: sessions.appVersion, os: sessions.os }).from(sessions).where(eq(sessions.userId, user.id)),
      myAchievements(db, user.id),
    ]);
    const list = infos.map((s) => {
      const d = devices.find((x) => x.id === s.id);
      return { ...s, appVersion: d?.appVersion ?? null, os: d?.os ?? null };
    });
    return html(reply, AccountPage({ user, sessions: list, unlocked: achievements.unlocked }) as string);
  });

  app.get('/reset/:code', async (req, reply) => {
    const code = (req.params as { code: string }).code.slice(0, 100);
    const [row] = await db
      .select({ username: users.username })
      .from(passwordResets)
      .innerJoin(users, eq(users.id, passwordResets.userId))
      .where(and(activeReset(code), isNull(users.disabledAt)));
    return html(reply, ResetPage({ code, username: row?.username ?? null }) as string);
  });

  app.get('/admin', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    if (!req.user!.isAdmin) return notFound(req, reply);
    const [stats, userList, transcodes, activity, games, reports] = await Promise.all([
      getAdminStats(db, app.disk),
      listAdminUsers(db, config.baseUrl),
      getTranscodes(db, app.transcoder),
      getActivity(db),
      getGameShares(db),
      listFeedback(db),
    ]);
    return html(
      reply,
      AdminPage({ user: req.user!, stats, users: userList, transcodes, activity, games, feedback: reports }) as string,
    );
  });

  // Just the compression section, for the admin page's live refresh.
  app.get('/admin/transcodes', async (req, reply) => {
    if (!guard(req, reply)) return reply;
    if (!req.user!.isAdmin) return notFound(req, reply);
    return html(reply, TranscodeSection({ data: await getTranscodes(db, app.transcoder) }) as string);
  });

  app.setNotFoundHandler((req, reply) => {
    if (req.url.startsWith('/api/') || req.url.startsWith('/media/')) return reply.code(404).send({ error: 'not_found' });
    return notFound(req, reply);
  });
}
