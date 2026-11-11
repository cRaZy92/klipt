// My clips merges the clips on this PC with your uploads on the server. Pure logic, tested in
// myClips.test.ts; Library.svelte renders it.

import type { ClipView, GameCount, MyUpload, Sort } from "./api";

/** Where a clip is: everything, the clips on this PC, or the ones on the server. */
export type Place = "all" | "local" | "uploaded";

export type MyClip =
  | { kind: "local"; key: string; clip: ClipView; at: number; game: string | null; steamAppId: number | null }
  | { kind: "online"; key: string; upload: MyUpload; at: number; game: string | null; steamAppId: number | null };

/**
 * One entry per clip. A local clip uploaded as one of `uploads` is that upload, shown as the
 * local clip; an upload whose local file is gone shows as the online clip instead (it can still
 * be played or downloaded); uploads with no local copy are online clips.
 */
export function mergeClips(local: ClipView[], uploads: MyUpload[]): MyClip[] {
  const online = new Map(uploads.map((u) => [u.id, u]));
  const items: MyClip[] = [];
  for (const clip of local) {
    const upload = clip.serverClipId ? online.get(clip.serverClipId) : undefined;
    if (upload && clip.fileMissing) continue; // shown as the online clip below
    if (upload) online.delete(upload.id);
    items.push({ kind: "local", key: `l:${clip.id}`, clip, at: clip.createdAt, game: clip.gameName, steamAppId: clip.steamAppId });
  }
  for (const upload of online.values()) {
    items.push({
      kind: "online",
      key: `s:${upload.id}`,
      upload,
      at: Date.parse(upload.createdAt) || 0,
      game: upload.gameName,
      steamAppId: upload.steamAppId,
    });
  }
  return items;
}

export function isUploaded(item: MyClip): boolean {
  return item.kind === "online" || !!item.clip.shareUrl;
}

export function matchesPlace(item: MyClip, place: Place): boolean {
  if (place === "local") return item.kind === "local";
  if (place === "uploaded") return isUploaded(item);
  return true;
}

/** The clips to show: in `place`, of `game` (null: any), in `sort` order. */
export function visibleClips(items: MyClip[], place: Place, game: string | null, sort: Sort): MyClip[] {
  const dir = sort === "newestFirst" ? -1 : 1;
  return items
    .filter((i) => matchesPlace(i, place) && (game === null || i.game === game))
    .sort((a, b) => dir * (a.at - b.at) || a.key.localeCompare(b.key));
}

/** Clips per game among `items`, most first (the game chips). Unknown games aren't a chip. */
export function gameCounts(items: MyClip[]): GameCount[] {
  const counts = new Map<string, GameCount>();
  for (const item of items) {
    if (!item.game) continue;
    const found = counts.get(item.game);
    if (found) found.count += 1;
    else counts.set(item.game, { name: item.game, steamAppId: item.steamAppId, count: 1 });
  }
  return [...counts.values()].sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
}
