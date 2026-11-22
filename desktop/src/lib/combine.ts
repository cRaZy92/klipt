// "Combine": several clips from My clips joined in order into one new clip (made by the backend).

import { editedLength } from "./cuts";

/** The most clips one combine takes (the backend's limit too). */
export const MAX_COMBINE = 50;

type Combinable = {
  id: string;
  kind: "clip" | "screenshot";
  fileMissing: boolean;
  createdAt: number;
  gameName: string | null;
};

/** The selected clips that can be combined (clips, not screenshots, with their file), oldest first. */
export function combinable<T extends Combinable>(clips: readonly T[]): T[] {
  return clips.filter((c) => c.kind === "clip" && !c.fileMissing).sort((a, b) => a.createdAt - b.createdAt);
}

/** `list` with the item at `from` moved one step (`by` = -1 up, 1 down); the same list at an end. */
export function moveItem<T>(list: readonly T[], from: number, by: -1 | 1): T[] {
  const to = from + by;
  if (from < 0 || from >= list.length || to < 0 || to >= list.length) return [...list];
  const next = [...list];
  [next[from], next[to]] = [next[to], next[from]];
  return next;
}

/** The suggested title: "<Game> highlights" when every clip is from one game, else "Highlights". */
export function combinedTitle(clips: readonly { gameName: string | null }[]): string {
  const games = new Set(clips.map((c) => c.gameName));
  const [only] = games;
  return games.size === 1 && only ? `${only} highlights` : "Highlights";
}

/** How long the combined clip will be: every clip's edited length, added up. */
export function combinedLength(
  clips: readonly { durationMs: number; trimStartMs: number | null; trimEndMs: number | null; removedRanges: { startMs: number; endMs: number }[] }[],
): number {
  return clips.reduce((sum, c) => sum + editedLength(c, c.durationMs), 0);
}
