import { and, asc, desc, eq, inArray, sql } from 'drizzle-orm';
import { MAX_ALBUM_CLIPS, type AlbumSummary } from '@klipt/shared';
import type { Db } from '../db/client.js';
import { albumClips, albums, clips, users } from '../db/schema.js';
import { displayNameOf, getClip, type ClipWithOwner } from '../clips/repo.js';
import { newClipId } from '../clips/ids.js';
import { thumbnailUrl } from '../clips/routes.js';

export type AlbumRow = typeof albums.$inferSelect;

/** An album with its owner, as the repo hands it to routes and pages. */
export interface AlbumWithOwner {
  album: AlbumRow;
  owner: { username: string; displayName: string };
  clipCount: number;
  /** The first clip's id and media version (for its thumbnail); null while empty. */
  cover: { id: string; mediaVersion: number; thumbVersion: number } | null;
  hasClip?: boolean;
}

/** Too many clips for one album. */
export class AlbumFullError extends Error {}
/** A clip id that doesn't exist. */
export class UnknownClipError extends Error {}

const clipCountExpr = sql<number>`(select count(*) from ${albumClips} ac where ac.album_id = ${albums.id})::int`;
/** The first clip, as `id|mediaVersion|thumbVersion`. */
const coverExpr = sql<string | null>`(select c.id || '|' || c.media_version || '|' || c.thumb_version from ${albumClips} ac join ${clips} c on c.id = ac.clip_id where ac.album_id = ${albums.id} order by ac.position, ac.added_at limit 1)`;

function parseCover(raw: string | null): AlbumWithOwner['cover'] {
  if (!raw) return null;
  const [id, version, thumb] = raw.split('|');
  return { id: id!, mediaVersion: Number(version) || 0, thumbVersion: Number(thumb) || 0 };
}

const summaryColumns = (clipId?: string) => ({
  album: albums,
  owner: { username: users.username, displayName: displayNameOf(users) },
  clipCount: clipCountExpr,
  cover: coverExpr,
  hasClip: clipId
    ? sql<boolean>`exists (select 1 from ${albumClips} ac where ac.album_id = ${albums.id} and ac.clip_id = ${clipId})`
    : sql<boolean>`false`,
});

/** The API shape of an album. */
export function toAlbumSummary(a: AlbumWithOwner, baseUrl: string): AlbumSummary {
  return {
    id: a.album.id,
    title: a.album.title,
    owner: a.owner,
    clipCount: a.clipCount,
    coverUrl: a.cover ? thumbnailUrl(a.cover, baseUrl) : null,
    shareUrl: `${baseUrl}/a/${a.album.id}`,
    createdAt: a.album.createdAt.toISOString(),
    updatedAt: a.album.updatedAt.toISOString(),
    ...(a.hasClip !== undefined ? { hasClip: a.hasClip } : {}),
  };
}

/** `userId`'s albums, most recently changed first; with `clipId`, each says whether it holds that clip. */
export async function listAlbums(db: Db, userId: string, clipId?: string): Promise<AlbumWithOwner[]> {
  const rows = await db
    .select(summaryColumns(clipId))
    .from(albums)
    .innerJoin(users, eq(users.id, albums.userId))
    .where(eq(albums.userId, userId))
    .orderBy(desc(albums.updatedAt), desc(albums.createdAt));
  return rows.map((r) => ({
    ...r,
    clipCount: Number(r.clipCount),
    cover: parseCover(r.cover),
    ...(clipId ? { hasClip: !!r.hasClip } : { hasClip: undefined }),
  }));
}

/** One album (anyone with its id may see it), or null. */
export async function getAlbum(db: Db, id: string): Promise<AlbumWithOwner | null> {
  const [row] = await db.select(summaryColumns()).from(albums).innerJoin(users, eq(users.id, albums.userId)).where(eq(albums.id, id));
  return row ? { ...row, clipCount: Number(row.clipCount), cover: parseCover(row.cover), hasClip: undefined } : null;
}

/** The clips of album `id`, in order; `viewerId` gets their stars. */
export async function albumClipList(db: Db, id: string, viewerId?: string | null): Promise<ClipWithOwner[]> {
  const ids = await db
    .select({ clipId: albumClips.clipId })
    .from(albumClips)
    .where(eq(albumClips.albumId, id))
    .orderBy(asc(albumClips.position), asc(albumClips.addedAt));
  const found = await Promise.all(ids.map((r) => getClip(db, r.clipId, viewerId)));
  return found.filter((c): c is ClipWithOwner => c !== null);
}

/** Throws UnknownClipError unless every id in `clipIds` is a clip. */
async function assertClips(db: Pick<Db, 'select'>, clipIds: string[]): Promise<void> {
  if (clipIds.length === 0) return;
  const rows = await db.select({ id: clips.id }).from(clips).where(inArray(clips.id, clipIds));
  if (rows.length !== new Set(clipIds).size) throw new UnknownClipError('unknown clip');
}

/** A new album of `userId` titled `title`, holding `clipIds` in that order. */
export async function createAlbum(db: Db, userId: string, title: string, clipIds: string[] = []): Promise<string> {
  const unique = [...new Set(clipIds)];
  if (unique.length > MAX_ALBUM_CLIPS) throw new AlbumFullError('album full');
  await assertClips(db, unique);
  const id = newClipId();
  await db.transaction(async (tx) => {
    await tx.insert(albums).values({ id, userId, title });
    if (unique.length > 0) await tx.insert(albumClips).values(unique.map((clipId, position) => ({ albumId: id, clipId, position })));
  });
  return id;
}

const touch = (db: Pick<Db, 'update'>, id: string) => db.update(albums).set({ updatedAt: new Date() }).where(eq(albums.id, id));

/** Adds `clipId` at the end of album `id` (already in it: nothing changes). */
export async function addAlbumClip(db: Db, id: string, clipId: string): Promise<void> {
  await assertClips(db, [clipId]);
  await db.transaction(async (tx) => {
    // Serializes adds to one album, so two at once can't both pass the size check.
    await tx.select({ id: albums.id }).from(albums).where(eq(albums.id, id)).for('update');
    const [state] = await tx
      .select({
        count: sql<number>`count(*)::int`,
        last: sql<number>`coalesce(max(${albumClips.position}), -1)::int`,
        has: sql<boolean>`coalesce(bool_or(${albumClips.clipId} = ${clipId}), false)`,
      })
      .from(albumClips)
      .where(eq(albumClips.albumId, id));
    if (state!.has) return;
    if (Number(state!.count) >= MAX_ALBUM_CLIPS) throw new AlbumFullError('album full');
    await tx.insert(albumClips).values({ albumId: id, clipId, position: Number(state!.last) + 1 });
    await touch(tx, id);
  });
}

/** Takes `clipId` out of album `id`. */
export async function removeAlbumClip(db: Db, id: string, clipId: string): Promise<void> {
  await db.transaction(async (tx) => {
    const gone = await tx
      .delete(albumClips)
      .where(and(eq(albumClips.albumId, id), eq(albumClips.clipId, clipId)))
      .returning({ clipId: albumClips.clipId });
    if (gone.length > 0) await touch(tx, id);
  });
}

/** The album's clips in a new order: `clipIds` must be exactly the clips it holds. False otherwise. */
export async function reorderAlbum(db: Db, id: string, clipIds: string[]): Promise<boolean> {
  return db.transaction(async (tx) => {
    const now = await tx.select({ clipId: albumClips.clipId }).from(albumClips).where(eq(albumClips.albumId, id));
    const have = new Set(now.map((r) => r.clipId));
    if (clipIds.length !== have.size || new Set(clipIds).size !== have.size || !clipIds.every((c) => have.has(c))) return false;
    for (const [position, clipId] of clipIds.entries()) {
      await tx.update(albumClips).set({ position }).where(and(eq(albumClips.albumId, id), eq(albumClips.clipId, clipId)));
    }
    await touch(tx, id);
    return true;
  });
}

export async function renameAlbum(db: Db, id: string, title: string): Promise<void> {
  await db.update(albums).set({ title, updatedAt: new Date() }).where(eq(albums.id, id));
}

export async function deleteAlbum(db: Db, id: string): Promise<void> {
  await db.delete(albums).where(eq(albums.id, id));
}
