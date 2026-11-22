// Muted and turned-down parts of the audio lanes: one list for every track, sorted by track, then
// time, never overlapping on one track. The export (src-tauri/src/export.rs, `track_volume`) turns
// each track down by the same amounts at the same moments, with the same short fades.

import type { AudioRange } from "./api";
import { moveRange, resizeRange, roomAt } from "./ranges";
import type { TrimRange } from "./trim";

/** The shortest part (the core refuses shorter). */
export const MIN_AUDIO_PART_MS = 200;
/** How long a part added at the playhead is (shorter when something follows sooner). */
export const DEFAULT_AUDIO_PART_MS = 2000;
/** The levels a part can be set to, in the order the level button goes through them. */
export const PART_LEVELS = [0, 0.25, 0.5] as const;
/** How long the sound fades into and out of a part (at most half of it), as in the export. */
export const AUDIO_RAMP_MS = 30;

const byTrackThenTime = (a: AudioRange, b: AudioRange) => a.track - b.track || a.startMs - b.startMs;

/** Part `i`'s track's parts, and where they start in the list. */
function trackSlice(parts: AudioRange[], track: number): { first: number; own: AudioRange[] } {
  const first = parts.findIndex((p) => p.track === track);
  return { first: first < 0 ? parts.length : first, own: parts.filter((p) => p.track === track) };
}

/** `parts` with track `track`'s parts replaced by `own` (sorted). */
function withTrack(parts: AudioRange[], track: number, own: AudioRange[]): AudioRange[] {
  return [...parts.filter((p) => p.track !== track), ...own].sort(byTrackThenTime);
}

/** The parts on `track`, each with its index in the whole list. */
export function partsOn(parts: AudioRange[], track: number): { part: AudioRange; i: number }[] {
  return parts.flatMap((part, i) => (part.track === track ? [{ part, i }] : []));
}

/**
 * A new part on `track` at `ms` (`lengthMs` long, shorter to fit before the next part or the trim
 * end), muted, and its index; null when there's no room for MIN_AUDIO_PART_MS there.
 */
export function addPart(
  parts: AudioRange[],
  track: number,
  ms: number,
  trim: TrimRange,
  lengthMs = DEFAULT_AUDIO_PART_MS,
): { parts: AudioRange[]; index: number } | null {
  const { own } = trackSlice(parts, track);
  const room = roomAt(own, ms, trim, lengthMs, MIN_AUDIO_PART_MS);
  if (!room) return null;
  const part = { track, ...room, level: 0 };
  const next = withTrack(parts, track, [...own, part].sort(byTrackThenTime));
  return { parts: next, index: next.indexOf(part) };
}

/**
 * Part `i` stretched between `anchor` (where a drag started) and `ms` (where the pointer is),
 * either way round: at least MIN_AUDIO_PART_MS long, between its neighbours and the trim.
 */
export function spanPart(parts: AudioRange[], i: number, anchor: number, ms: number, trim: TrimRange): AudioRange[] {
  const p = parts[i];
  if (!p) return parts;
  const before = parts[i - 1]?.track === p.track ? parts[i - 1].endMs : 0;
  const after = parts[i + 1]?.track === p.track ? parts[i + 1].startMs : Infinity;
  const lo = Math.max(before, trim.start);
  const hi = Math.min(after, trim.end);
  const a = Math.round(anchor);
  const b = Math.min(hi, Math.max(lo, Math.round(ms)));
  let start = Math.min(a, b);
  let end = Math.max(a, b);
  // Too short: grow away from the anchor, as far as there is room.
  if (end - start < MIN_AUDIO_PART_MS) {
    if (b >= a) end = Math.min(hi, start + MIN_AUDIO_PART_MS);
    else start = Math.max(lo, end - MIN_AUDIO_PART_MS);
    if (end - start < MIN_AUDIO_PART_MS) return parts;
  }
  if (start === p.startMs && end === p.endMs) return parts;
  return parts.map((o, j) => (j === i ? { ...o, startMs: start, endMs: end } : o));
}

/** Part `i` moved to start at `startMs`, between its neighbours on its track and the trim. */
export function movePart(parts: AudioRange[], i: number, startMs: number, trim: TrimRange): AudioRange[] {
  const p = parts[i];
  if (!p) return parts;
  const { first, own } = trackSlice(parts, p.track);
  const moved = moveRange(own, i - first, startMs, trim);
  return moved === own ? parts : withTrack(parts, p.track, moved);
}

/** Part `i` with its `edge` moved to `ms`. */
export function resizePart(
  parts: AudioRange[],
  i: number,
  edge: "start" | "end",
  ms: number,
  trim: TrimRange,
): AudioRange[] {
  const p = parts[i];
  if (!p) return parts;
  const { first, own } = trackSlice(parts, p.track);
  const resized = resizeRange(own, i - first, edge, ms, trim, MIN_AUDIO_PART_MS);
  return resized === own ? parts : withTrack(parts, p.track, resized);
}

export function removePart(parts: AudioRange[], i: number): AudioRange[] {
  return parts.filter((_, j) => j !== i);
}

/** The level after `level` on the level button: Muted → 25% → 50% → Muted. */
export function nextLevel(level: number): number {
  const at = PART_LEVELS.findIndex((l) => Math.abs(l - level) < 1e-6);
  return PART_LEVELS[(at + 1) % PART_LEVELS.length];
}

export function setPartLevel(parts: AudioRange[], i: number, level: number): AudioRange[] {
  return parts.map((p, j) => (j === i ? { ...p, level } : p));
}

/** How a part's level reads on its block. */
export function levelLabel(level: number): string {
  return level <= 0 ? "Muted" : `${Math.round(level * 100)}%`;
}

export function sameParts(a: AudioRange[], b: AudioRange[]): boolean {
  return (
    a.length === b.length &&
    a.every((p, i) => p.track === b[i].track && p.startMs === b[i].startMs && p.endMs === b[i].endMs && p.level === b[i].level)
  );
}

function rampOf(p: AudioRange): number {
  return Math.min(AUDIO_RAMP_MS, Math.max(1, (p.endMs - p.startMs) / 2));
}

/** How loud a track with `parts` (its own only) plays at `ms`, 0–1, fades included. */
export function gainAt(parts: AudioRange[], ms: number): number {
  let drop = 0;
  for (const p of parts) {
    const r = rampOf(p);
    const fadeIn = Math.min(1, Math.max(0, (ms - p.startMs) / r));
    const fadeOut = Math.min(1, Math.max(0, (p.endMs - ms) / r));
    drop += (1 - p.level) * fadeIn * fadeOut;
  }
  return Math.max(0, 1 - drop);
}

/**
 * The gain of a track with `parts` (its own only) from `from` to `to` ms as points to join with
 * straight lines: exact, since the fades are straight. Always starts at `from` and ends at `to`.
 */
export function gainPoints(parts: AudioRange[], from: number, to: number): { ms: number; gain: number }[] {
  const knots = new Set([from, to]);
  for (const p of parts) {
    if (p.endMs <= from || p.startMs >= to) continue;
    const r = rampOf(p);
    for (const ms of [p.startMs, p.startMs + r, p.endMs - r, p.endMs]) {
      if (ms > from && ms < to) knots.add(ms);
    }
  }
  return [...knots].sort((a, b) => a - b).map((ms) => ({ ms, gain: gainAt(parts, ms) }));
}
