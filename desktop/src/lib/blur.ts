// Blurred parts in the editor: during each, the whole picture is blurred (as "Blur video" blurs
// the whole clip). The export (src-tauri/src/export.rs, `blur_filter`) blurs the same moments.

import type { BlurRange } from "./api";
import type { TrimRange } from "./trim";
import { moveRange, resizeRange, roomAt } from "./ranges";

/** The shortest a blurred part can be (MIN_BLUR_MS in src-tauri/src/library/mod.rs). */
export const MIN_BLUR_MS = 300;
/** How long a new blurred part is (shorter when something follows sooner). */
export const DEFAULT_BLUR_MS = 3000;

/** The index of the blurred part at `ms`, or −1. */
export function blurIndexAt(blurs: BlurRange[], ms: number): number {
  return blurs.findIndex((b) => ms >= b.startMs && ms < b.endMs);
}

/**
 * A new blurred part at `ms` (DEFAULT_BLUR_MS long, shorter to fit before the next one or the trim
 * end), or null when there's no room for MIN_BLUR_MS there.
 */
export function addBlurAt(blurs: BlurRange[], ms: number, trim: TrimRange): BlurRange[] | null {
  const room = roomAt(blurs, ms, trim, DEFAULT_BLUR_MS, MIN_BLUR_MS);
  if (!room) return null;
  return [...blurs, room].sort((a, b) => a.startMs - b.startMs);
}

/** Part `i` moved to start at `startMs`, keeping its length, between its neighbours and the trim. */
export function moveBlur(blurs: BlurRange[], i: number, startMs: number, trim: TrimRange): BlurRange[] {
  return moveRange(blurs, i, startMs, trim);
}

/** Part `i` with its `edge` moved to `ms`: at least MIN_BLUR_MS long, between its neighbours and the trim. */
export function resizeBlur(
  blurs: BlurRange[],
  i: number,
  edge: "start" | "end",
  ms: number,
  trim: TrimRange,
): BlurRange[] {
  return resizeRange(blurs, i, edge, ms, trim, MIN_BLUR_MS);
}

export function removeBlur(blurs: BlurRange[], i: number): BlurRange[] {
  return blurs.filter((_, j) => j !== i);
}

/** Whether two lists of blurred parts are the same. */
export function sameBlurs(a: BlurRange[], b: BlurRange[]): boolean {
  return a.length === b.length && a.every((r, i) => r.startMs === b[i].startMs && r.endMs === b[i].endMs);
}
