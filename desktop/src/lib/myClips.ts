// My clips merges the clips on this PC with your uploads on the server. Pure logic, tested in
// myClips.test.ts; Library.svelte renders it.

import type { IconName } from "../components/Icon.svelte";
import type { ClipView, GameCount, MyUpload, Sort } from "./api";

/** Where one clip is: only on this PC, on this PC and uploaded, or only uploaded. */
export type Where = "localOnly" | "both" | "online";

/** Each place's icon (on the filter and the card) and name. */
export const WHERE: Record<Where, { icon: IconName; label: string; title: string }> = {
  localOnly: { icon: "pc", label: "Only on this PC", title: "Only on this PC: not uploaded" },
  both: { icon: "cloudCheck", label: "On PC & uploaded", title: "On this PC and uploaded" },
  online: { icon: "cloud", label: "Only uploaded", title: "Uploaded; not on this PC" },
};

/** Which clips to show: everything, one place, or starred ones. */
export type Place = "all" | Where | "favorites";

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
    // Starred here or on the server (e.g. on the website): either counts.
    const shown = upload?.favorite && !clip.favorite ? { ...clip, favorite: true } : clip;
    items.push({ kind: "local", key: `l:${clip.id}`, clip: shown, at: clip.createdAt, game: clip.gameName, steamAppId: clip.steamAppId });
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

export function whereOf(item: MyClip): Where {
  if (item.kind === "online") return "online";
  return item.clip.shareUrl ? "both" : "localOnly";
}

/** Videos, screenshots or both. */
export type MediaKind = "all" | "clip" | "screenshot";

/** Whether an entry is a video clip or a screenshot (older servers know only clips). */
export function mediaKindOf(item: MyClip): "clip" | "screenshot" {
  return (item.kind === "local" ? item.clip.kind : item.upload.kind) ?? "clip";
}

export function matchesPlace(item: MyClip, place: Place): boolean {
  if (place === "favorites") return item.kind === "local" ? item.clip.favorite : !!item.upload.favorite;
  if (place === "all") return true;
  return whereOf(item) === place;
}

/** Lower case without accents, so "utek" finds "Útek". */
function fold(text: string): string {
  return text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** Every word of `search` appears in one of `fields` (case and accents ignored); blank matches all. */
export function matchesSearch(fields: (string | null | undefined)[], search: string): boolean {
  const words = fold(search).split(/\s+/).filter(Boolean);
  const haystack = fields.filter(Boolean).map((f) => fold(f!));
  return words.every((w) => haystack.some((f) => f.includes(w)));
}

function titleOf(item: MyClip): string {
  return item.kind === "local" ? item.clip.title : item.upload.title;
}

/** The clips to show: in `place`, of `game` (null: any), matching `search`, in `sort` order. */
export function visibleClips(
  items: MyClip[],
  place: Place,
  game: string | null,
  sort: Sort,
  search = "",
  media: MediaKind = "all",
): MyClip[] {
  const dir = sort === "newestFirst" ? -1 : 1;
  return items
    .filter(
      (i) =>
        matchesPlace(i, place) &&
        (game === null || i.game === game) &&
        (media === "all" || mediaKindOf(i) === media) &&
        matchesSearch([titleOf(i), i.game], search),
    )
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

/**
 * The game chips: clips per game among what the other filters (place, media, search) let
 * through, so a chip never promises clips it then doesn't show. The picked game keeps its chip
 * (at 0) so it can still be seen and unpicked while a search hides all of it.
 */
export function gameChips(
  items: MyClip[],
  place: Place,
  game: string | null,
  search = "",
  media: MediaKind = "all",
): GameCount[] {
  const chips = gameCounts(visibleClips(items, place, null, "newestFirst", search, media));
  if (game !== null && !chips.some((c) => c.name === game)) {
    const steamAppId = items.find((i) => i.game === game)?.steamAppId ?? null;
    chips.push({ name: game, steamAppId, count: 0 });
  }
  return chips;
}
