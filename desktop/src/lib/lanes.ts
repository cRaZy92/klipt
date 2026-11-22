// Pure math for the editor's track lanes: the time ruler, muting, and the shading every lane
// draws at the same positions as the video lane (TrimBar).

import type { Piece } from "./cuts";
import { formatDuration } from "./format";
import { fullView, viewPct, viewSpan, type TimeView } from "./timelineView";
import { formatClock, type TrimRange } from "./trim";

/** A stretch of a lane, in percent of its width. */
export interface Span {
  left: number;
  width: number;
}

/** A removed part on a lane; open on a side where it runs off a zoomed-in screen. */
export interface CutSpan extends Span {
  openLeft: boolean;
  openRight: boolean;
}

/** Finer steps for a zoomed-in timeline, and coarser ones for long recordings. */
const FINE_STEPS = [100, 200, 500, 1000, 2000];
const LONG_STEPS = [30_000, 60_000, 120_000, 300_000, 600_000, 900_000, 1_800_000, 3_600_000];

/**
 * How far apart the ruler's labels are for a timeline showing `spanMs`: under 10 s, the finest of
 * 0.1–2 s that gives at most 10 labels; 5 s under 30 s; 10 s up to 3 min; over that, the finest of
 * 30 s–1 h that gives at most 20.
 */
export function rulerStep(spanMs: number): number {
  if (spanMs < 10_000) return FINE_STEPS.find((s) => spanMs / s <= 10) ?? 2000;
  if (spanMs < 30_000) return 5000;
  if (spanMs <= 180_000) return 10_000;
  return LONG_STEPS.find((s) => spanMs / s <= 20) ?? LONG_STEPS[LONG_STEPS.length - 1];
}

/**
 * The ruler's labels over `view` (the whole clip by default), one per step from 0 (none at the
 * clip's very end, where it wouldn't fit). Steps under a second label tenths.
 */
export function rulerTicks(
  durationMs: number,
  view: TimeView = fullView(durationMs),
): { ms: number; pct: number; label: string }[] {
  const span = view.to - view.from;
  if (!(durationMs > 0) || !(span > 0)) return [];
  const step = rulerStep(span);
  const label = step < 1000 ? formatClock : formatDuration;
  const ticks = [];
  for (let ms = Math.ceil(view.from / step) * step; ms < Math.min(view.to, durationMs); ms += step) {
    ticks.push({ ms, pct: viewPct(ms, view), label: label(ms) });
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

/**
 * What a lane dims (outside the trim) and shades (removed parts inside it), as TrimBar does, as
 * far as it is inside `view` (the whole clip by default).
 */
export function laneShading(
  duration: number,
  trim: TrimRange,
  pieces: Piece[],
  view: TimeView = fullView(duration),
): { dims: Span[]; cuts: CutSpan[] } {
  if (!(duration > 0)) return { dims: [], cuts: [] };
  const span = (from: number, to: number) => viewSpan(from, to, view);
  const dims = [span(0, trim.start), span(trim.end, duration)].filter((d): d is Span => d !== null);
  const cuts = pieces.flatMap((p) => {
    const from = Math.max(p.start, trim.start);
    const to = Math.min(p.end, trim.end);
    const part = p.removed ? span(from, to) : null;
    return part ? [{ ...part, openLeft: from < view.from, openRight: to > view.to }] : [];
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
