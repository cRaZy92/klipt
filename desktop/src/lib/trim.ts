// Pure trim math for the editor's TrimBar: times are whole milliseconds, positions are pixels
// from the bar's left edge.

export interface TrimRange {
  start: number;
  end: number;
}

/** The shortest cut the editor allows. */
export const MIN_TRIM_MS = 500;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * A valid trim: 0 ≤ start < end ≤ duration and end − start ≥ minLen (the whole clip if it is
 * shorter than that). `keep` is the handle that stays put when the range must grow: the end
 * handle while it is being dragged, otherwise the start.
 */
export function clampTrim(
  start: number,
  end: number,
  duration: number,
  minLen = MIN_TRIM_MS,
  keep: "start" | "end" = "start",
): TrimRange {
  const total = Number.isFinite(duration) && duration > 0 ? Math.round(duration) : 0;
  const min = Math.min(Math.max(0, minLen), total);
  let s = Number.isFinite(start) ? Math.round(start) : 0;
  let e = Number.isFinite(end) ? Math.round(end) : total;
  if (keep === "start") {
    s = clamp(s, 0, total - min);
    e = clamp(e, s + min, total);
  } else {
    e = clamp(e, min, total);
    s = clamp(s, 0, e - min);
  }
  return { start: s, end: e };
}

/** Where `ms` sits on a bar `width` pixels wide. */
export function msToX(ms: number, duration: number, width: number): number {
  if (!(duration > 0) || !(width > 0) || !Number.isFinite(ms)) return 0;
  return (clamp(ms, 0, duration) / duration) * width;
}

/** The time at `x` pixels on a bar `width` pixels wide, in whole milliseconds. */
export function xToMs(x: number, duration: number, width: number): number {
  if (!(duration > 0) || !(width > 0) || !Number.isFinite(x)) return 0;
  return Math.round((clamp(x, 0, width) / width) * duration);
}

/** The trim as the library stores it: an end left at the clip's edge is stored as no trim. */
export function savedTrim(trim: TrimRange, duration: number): { start: number | null; end: number | null } {
  return { start: trim.start > 0 ? trim.start : null, end: trim.end < duration ? trim.end : null };
}

/** A position in the clip, to the tenth of a second (rounded down): 65340 → "1:05.3". */
export function formatClock(ms: number): string {
  const tenths = Number.isFinite(ms) && ms > 0 ? Math.floor(ms / 100) : 0;
  const minutes = Math.floor(tenths / 600);
  const seconds = Math.floor((tenths % 600) / 10);
  return `${minutes}:${String(seconds).padStart(2, "0")}.${tenths % 10}`;
}
