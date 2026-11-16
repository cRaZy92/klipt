// Pure helpers for playing the kept parts of a clip back to back on one audio clock.

import { playFrom } from "./cuts";
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

/** What a video seek is assumed to take before any was measured. */
export const SEEK_LATENCY_START_MS = 150;
const SEEK_LATENCY_MAX_MS = 1500;
/** How much each measured seek moves the average. */
const SEEK_LATENCY_WEIGHT = 0.3;

/** The moving average of how long a video seek takes, after one that took `sampleMs`. */
export function seekLatency(averageMs: number, sampleMs: number): number {
  const clamp = (ms: number) => Math.min(SEEK_LATENCY_MAX_MS, Math.max(0, ms));
  return clamp(averageMs + (clamp(sampleMs) - averageMs) * SEEK_LATENCY_WEIGHT);
}

/**
 * How to pull the video back to the audio: nothing, a rate change (stronger the further off), or
 * a seek. A seek aims where the audio will be once it lands (`seekMs` ahead), within the part
 * playing (it ends at `segmentEndMs`).
 */
export function videoCorrection(
  videoMs: number,
  audioMs: number,
  seekMs: number,
  segmentEndMs: number,
): { rate: number } | { seekMs: number } | null {
  const drift = audioMs - videoMs;
  const abs = Math.abs(drift);
  if (abs <= 80) return null;
  if (abs <= 250) return { rate: drift > 0 ? 1.03 : 0.97 };
  if (abs <= 500) return { rate: drift > 0 ? 1.1 : 0.9 };
  return { seekMs: Math.min(audioMs + seekMs, segmentEndMs) };
}

/**
 * Where to seek the video ahead of a cut, so it lands on the next part as the audio gets there:
 * once the audio (at `audioMs` in `seg`) is a seek's time (`seekMs`) from the cut. Null while
 * it's further off, or where nothing is cut (the next part follows on, or there is none).
 */
export function cutSeekAhead(audioMs: number, seg: Segment, next: Segment | null, seekMs: number): number | null {
  if (!next || next.from === seg.to) return null;
  const remaining = seg.to - audioMs;
  if (remaining > seekMs) return null;
  return Math.min(next.from + Math.max(0, seekMs - remaining), next.to);
}

/**
 * With no audio clock (the video plays on its own): where the video at `ms` must jump to skip
 * removed parts and loop, or null to play on. A frame up to 50 ms before a part's start is where
 * a seek to it can land: no jump for that.
 */
export function videoLedJump(ms: number, ranges: TrimRange[]): number | null {
  const to = playFrom(ms, ranges);
  if (to === ms || (to > ms && to - ms <= 50)) return null;
  return to;
}

/** The audio engine starts what it's asked to play this long after it's asked (ms). */
export const AUDIO_LEAD_MS = 50;

/**
 * Whether the video, asked to play from `startMs`, is really playing now (at `videoMs`): its
 * position has moved on and it isn't seeking. WebKitGTK sometimes never sends `playing` (e.g.
 * pressed right after a scrub), so the editor watches the position instead of waiting for it.
 */
export function videoStarted(startMs: number, videoMs: number, seeking: boolean): boolean {
  return !seeking && videoMs > startMs + 1;
}

/** Where the sound starts so it lines up with a video now at `videoMs` (see AUDIO_LEAD_MS). */
export function audioStartMs(videoMs: number): number {
  return videoMs + AUDIO_LEAD_MS;
}
