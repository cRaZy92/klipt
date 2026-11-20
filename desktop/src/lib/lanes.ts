// Pure math for the editor's track lanes: the time ruler, muting, and the shading every lane
// draws at the same positions as the video lane (TrimBar).

import type { Piece } from "./cuts";
import { formatDuration } from "./format";
import { msToX, type TrimRange } from "./trim";

/** A stretch of a lane, in percent of its width. */
export interface Span {
  left: number;
  width: number;
}

/** How far apart the ruler's labels are: 5 s under 30 s, 30 s over 3 min, else 10 s. */
export function rulerStep(durationMs: number): number {
  if (durationMs < 30_000) return 5000;
  if (durationMs > 180_000) return 30_000;
  return 10_000;
}

/** The ruler's labels from 0, one per step (none at the clip's very end, where it wouldn't fit). */
export function rulerTicks(durationMs: number): { ms: number; pct: number; label: string }[] {
  if (!(durationMs > 0)) return [];
  const step = rulerStep(durationMs);
  const ticks = [];
  for (let ms = 0; ms < durationMs; ms += step) {
    ticks.push({ ms, pct: msToX(ms, durationMs, 100), label: formatDuration(ms) });
  }
  return ticks;
}

/**
 * The mute button: a playing track goes to 0 and its level is remembered; a track at 0 comes
 * back to the remembered level (100 when there is none).
 */
export function toggleMute(volume: number, remembered: number | null): { volume: number; remembered: number | null } {
  if (volume > 0) return { volume: 0, remembered: volume };
  return { volume: remembered && remembered > 0 ? remembered : 100, remembered: null };
}

/** What a lane dims (outside the trim) and shades (removed parts inside it), as TrimBar does. */
export function laneShading(duration: number, trim: TrimRange, pieces: Piece[]): { dims: Span[]; cuts: Span[] } {
  const pct = (ms: number) => msToX(ms, duration, 100);
  const dims = [
    { left: 0, width: pct(trim.start) },
    { left: pct(trim.end), width: 100 - pct(trim.end) },
  ];
  const cuts = pieces.flatMap((p) => {
    const from = Math.max(p.start, trim.start);
    const to = Math.min(p.end, trim.end);
    return p.removed && to > from && duration > 0 ? [{ left: pct(from), width: pct(to) - pct(from) }] : [];
  });
  return { dims, cuts };
}

/**
 * How a block on the Zoom or Subtitles lane is laid out at `widthPx` wide: how wide its resize
 * edges are (at most 10 px, and never more than a quarter of it, so its middle still moves it and
 * takes a double-click), and whether its text and its remove button fit.
 */
export function blockLayout(widthPx: number): { edgePx: number; showText: boolean; showRemove: boolean } {
  return { edgePx: Math.min(10, widthPx / 4), showText: widthPx >= 28, showRemove: widthPx >= 72 };
}
