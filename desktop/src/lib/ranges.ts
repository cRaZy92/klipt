// Moving and resizing timed blocks on a lane (zooms, subtitles): sorted, never overlapping, kept
// between their neighbours and the trim.

import type { TrimRange } from "./trim";

export interface TimeRange {
  startMs: number;
  endMs: number;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/**
 * How far block `i` may reach: up to its neighbours and the trim. A block already outside the
 * trim (the trim moved past it) may stay where it is.
 */
function bounds(ranges: TimeRange[], i: number, trim: TrimRange): { lo: number; hi: number } {
  const r = ranges[i];
  const prev = ranges[i - 1];
  const next = ranges[i + 1];
  return {
    lo: Math.max(prev ? prev.endMs : 0, Math.min(trim.start, r.startMs)),
    hi: Math.min(next ? next.startMs : Infinity, Math.max(trim.end, r.endMs)),
  };
}

/** Block `i` moved to start at `startMs`, keeping its length, between its neighbours and the trim. */
export function moveRange<T extends TimeRange>(ranges: T[], i: number, startMs: number, trim: TrimRange): T[] {
  const r = ranges[i];
  if (!r) return ranges;
  const { lo, hi } = bounds(ranges, i, trim);
  const len = r.endMs - r.startMs;
  if (hi - lo < len) return ranges;
  const start = clamp(Math.round(startMs), lo, hi - len);
  if (start === r.startMs) return ranges;
  return ranges.map((o, j) => (j === i ? { ...o, startMs: start, endMs: start + len } : o));
}

/** Block `i` with its `edge` moved to `ms`: at least `minMs` long, between its neighbours and the trim. */
export function resizeRange<T extends TimeRange>(
  ranges: T[],
  i: number,
  edge: "start" | "end",
  ms: number,
  trim: TrimRange,
  minMs: number,
): T[] {
  const r = ranges[i];
  if (!r) return ranges;
  const { lo, hi } = bounds(ranges, i, trim);
  const at = Math.round(ms);
  const next =
    edge === "start"
      ? { ...r, startMs: clamp(at, lo, r.endMs - minMs) }
      : { ...r, endMs: clamp(at, r.startMs + minMs, hi) };
  if (next.startMs === r.startMs && next.endMs === r.endMs) return ranges;
  return ranges.map((o, j) => (j === i ? next : o));
}

/**
 * Where a new block at `ms` would go: `defaultMs` long, shorter to fit before the next block or
 * the trim end, or null when there's no room for `minMs` there.
 */
export function roomAt(ranges: TimeRange[], ms: number, trim: TrimRange, defaultMs: number, minMs: number): TimeRange | null {
  const at = Math.round(ms);
  if (at < trim.start || at >= trim.end || ranges.some((r) => at >= r.startMs && at < r.endMs)) return null;
  const next = ranges.find((r) => r.startMs >= at);
  const end = Math.min(at + defaultMs, next ? next.startMs : Infinity, trim.end);
  return end - at < minMs ? null : { startMs: at, endMs: end };
}
