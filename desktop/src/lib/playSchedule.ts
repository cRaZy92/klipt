// Pure helpers for playing the kept parts of a clip back to back on one audio clock.

import type { TrimRange } from "./trim";

/** `at` is seconds after play starts; `from`/`to` are ms in the clip. */
export type Segment = { at: number; from: number; to: number };

/** The kept parts from `fromMs` to the end, back to back. Wraps to the first part past the end. */
export function schedule(fromMs: number, kept: TrimRange[]): Segment[] {
  if (kept.length === 0) return [];
  const last = kept[kept.length - 1];
  const start = fromMs >= last.end ? kept[0].start : fromMs;
  const segments: Segment[] = [];
  let elapsedMs = 0;
  for (const r of kept) {
    if (r.end <= start) continue;
    const from = Math.max(r.start, start);
    segments.push({ at: elapsedMs / 1000, from, to: r.end });
    elapsedMs += r.end - from;
  }
  return segments;
}

/** The segment that `elapsedS` seconds after play starts falls in, or null past the end. */
export function segmentAt(elapsedS: number, segments: Segment[]): Segment | null {
  if (segments.length === 0) return null;
  if (elapsedS < 0) return segments[0];
  for (const s of segments) {
    if (elapsedS < s.at + (s.to - s.from) / 1000) return s;
  }
  return null;
}

/** The clip position `elapsedS` seconds after play starts, or null past the end (the caller loops). */
export function clipMsAt(elapsedS: number, segments: Segment[]): number | null {
  const s = segmentAt(elapsedS, segments);
  if (!s) return null;
  return s.from + Math.max(0, elapsedS - s.at) * 1000;
}

/** How to pull the video back to the audio: nothing, a slight rate change, or a seek. */
export function videoCorrection(videoMs: number, audioMs: number): { rate: number } | { seekMs: number } | null {
  const drift = audioMs - videoMs;
  const abs = Math.abs(drift);
  if (abs <= 80) return null;
  if (abs <= 500) return { rate: drift > 0 ? 1.03 : 0.97 };
  return { seekMs: audioMs };
}
