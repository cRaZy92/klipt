// Pure math for removing parts of a clip in the editor. The timeline is a list of pieces that
// always covers 0..duration without gaps; the boundaries between them are the split lines. The
// trim handles only hide pieces: what is outside the trim is kept in the list but never saved.

import type { CutRange } from "./api";
import { MIN_TRIM_MS, type TrimRange } from "./trim";

export interface Piece {
  start: number;
  end: number;
  removed: boolean;
}

/** One kept piece: the clip with no splits. */
export function wholeClip(duration: number): Piece[] {
  return [{ start: 0, end: Math.max(0, Math.round(duration)), removed: false }];
}

/** The pieces saved `ranges` (sorted) make of a clip `duration` long. */
export function piecesFromRanges(ranges: CutRange[], duration: number): Piece[] {
  const total = Math.max(0, Math.round(duration));
  const pieces: Piece[] = [];
  let at = 0;
  for (const r of ranges) {
    const start = Math.max(at, Math.min(r.startMs, total));
    const end = Math.max(start, Math.min(r.endMs, total));
    if (start > at) pieces.push({ start: at, end: start, removed: false });
    if (end > start) pieces.push({ start, end, removed: true });
    at = end;
  }
  if (total > at || pieces.length === 0) pieces.push({ start: at, end: total, removed: false });
  return pieces;
}

/** Splits the piece under `ms`; null when `ms` is outside the trim or a half would be too short. */
export function splitAt(pieces: Piece[], ms: number, trim: TrimRange): Piece[] | null {
  const at = Math.round(ms);
  if (!(at > trim.start && at < trim.end)) return null;
  const i = pieces.findIndex((p) => at > p.start && at < p.end);
  if (i < 0) return null;
  const p = pieces[i];
  if (at - p.start < MIN_TRIM_MS || p.end - at < MIN_TRIM_MS) return null;
  return [...pieces.slice(0, i), { ...p, end: at }, { ...p, start: at }, ...pieces.slice(i + 1)];
}

/** Moves the split between pieces `i` and `i + 1`, keeping both at least MIN_TRIM_MS long. */
export function moveSplit(pieces: Piece[], i: number, ms: number): Piece[] {
  const a = pieces[i];
  const b = pieces[i + 1];
  if (!a || !b) return pieces;
  const at = Math.min(b.end - MIN_TRIM_MS, Math.max(a.start + MIN_TRIM_MS, Math.round(ms)));
  if (at === a.end) return pieces;
  return pieces.map((p, j) => (j === i ? { ...p, end: at } : j === i + 1 ? { ...p, start: at } : p));
}

/** Removes the split between pieces `i` and `i + 1`; the merged piece is removed only if both were. */
export function deleteSplit(pieces: Piece[], i: number): Piece[] {
  const a = pieces[i];
  const b = pieces[i + 1];
  if (!a || !b) return pieces;
  return [...pieces.slice(0, i), { start: a.start, end: b.end, removed: a.removed && b.removed }, ...pieces.slice(i + 2)];
}

/** Switches piece `i` between kept and removed. */
export function toggle(pieces: Piece[], i: number): Piece[] {
  return pieces.map((p, j) => (j === i ? { ...p, removed: !p.removed } : p));
}

/** The kept parts inside the trim, in order. */
export function keptRanges(pieces: Piece[], trim: TrimRange): TrimRange[] {
  const kept: TrimRange[] = [];
  for (const p of pieces) {
    if (p.removed) continue;
    const start = Math.max(p.start, trim.start);
    const end = Math.min(p.end, trim.end);
    if (end <= start) continue;
    const last = kept[kept.length - 1];
    if (last && last.end === start) last.end = end;
    else kept.push({ start, end });
  }
  return kept;
}

/** How long the kept parts inside the trim are, in ms. */
export function keptLength(pieces: Piece[], trim: TrimRange): number {
  return keptRanges(pieces, trim).reduce((sum, k) => sum + k.end - k.start, 0);
}

/** False when switching piece `i` off would leave nothing kept inside the trim. */
export function canRemove(pieces: Piece[], i: number, trim: TrimRange): boolean {
  if (pieces[i]?.removed) return true;
  return keptRanges(toggle(pieces, i), trim).length > 0;
}

/** The removed parts as saved: clipped to the trim, neighbours merged. */
export function removedRanges(pieces: Piece[], trim: TrimRange): CutRange[] {
  const ranges: CutRange[] = [];
  for (const p of pieces) {
    if (!p.removed) continue;
    const startMs = Math.max(p.start, trim.start);
    const endMs = Math.min(p.end, trim.end);
    if (endMs <= startMs) continue;
    const last = ranges[ranges.length - 1];
    if (last && last.endMs === startMs) last.endMs = endMs;
    else ranges.push({ startMs, endMs });
  }
  return ranges;
}

/** Where playback from `ms` starts: `ms` itself when kept, else the next kept time (looping). */
export function playFrom(ms: number, kept: TrimRange[]): number {
  if (kept.length === 0) return ms;
  for (const k of kept) {
    if (ms < k.start) return k.start;
    if (ms < k.end) return ms;
  }
  return kept[0].start;
}

/** A frame on a kept part: `ms` when kept, else the next kept time, or the last kept frame. */
export function clampToKept(ms: number, kept: TrimRange[]): number {
  if (kept.length === 0) return ms;
  for (const k of kept) {
    if (ms < k.start) return k.start;
    if (ms < k.end) return ms;
  }
  return kept[kept.length - 1].end - 1;
}

export function sameRanges(a: CutRange[], b: CutRange[]): boolean {
  return a.length === b.length && a.every((r, i) => r.startMs === b[i].startMs && r.endMs === b[i].endMs);
}
