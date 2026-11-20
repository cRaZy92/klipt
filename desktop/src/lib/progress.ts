// Progress details for long waits: an upload's size, speed and time left, and the steps of
// opening a clip in the editor.

import { formatSize } from "./format";

/** Bytes per second over a sliding window of (time, bytes so far) samples. */
export class RateMeter {
  private samples: { t: number; bytes: number }[] = [];

  constructor(private windowMs = 4000) {}

  add(t: number, bytes: number) {
    const last = this.samples.at(-1);
    // Fewer bytes than before: part of it is being sent again, so the old samples mean nothing.
    if (last && bytes < last.bytes) this.samples = [];
    this.samples.push({ t, bytes });
    while (this.samples.length > 2 && this.samples[1].t <= t - this.windowMs) this.samples.shift();
  }

  /** Null until there's at least a second of samples to go by. */
  rate(): number | null {
    const first = this.samples[0];
    const last = this.samples.at(-1);
    if (!first || !last || last.t - first.t < 1000) return null;
    return ((last.bytes - first.bytes) * 1000) / (last.t - first.t);
  }
}

/** "6.2 MB/s", "13 MB/s", "300 KB/s". */
export function formatRate(bytesPerSecond: number): string {
  if (bytesPerSecond < 1024 * 1024) return `${Math.round(bytesPerSecond / 1024)} KB/s`;
  return `${formatSize(bytesPerSecond)}/s`;
}

/** "5 s left", "3 min left". */
export function timeLeft(seconds: number): string {
  const s = Math.max(1, Math.ceil(seconds));
  return s < 60 ? `${s} s left` : `${Math.round(s / 60)} min left`;
}

/** "12 MB of 38 MB · 2 MB/s · 13 s left"; just the sizes while the speed isn't known yet. */
export function transferDetail(sent: number, total: number, rate: number | null): string {
  const size = `${formatSize(sent)} of ${formatSize(total)}`;
  if (!rate || rate <= 0) return size;
  return `${size} · ${formatRate(rate)} · ${timeLeft((total - sent) / rate)}`;
}

/** An `editor-progress` event, without its clip id. */
export interface EditorProgress {
  step: "reading" | "audio";
  track: number;
  tracks: number;
  fraction: number;
}

/** What the editor shows while it prepares a clip; `fraction` is null when it can't be measured. */
export function editorStep(p: EditorProgress | null): { label: string; fraction: number | null } {
  if (!p) return { label: "Opening the clip…", fraction: null };
  if (p.step === "reading" || p.tracks <= 0) return { label: "Reading the clip…", fraction: null };
  const label = p.tracks > 1 ? `Extracting audio (track ${p.track + 1} of ${p.tracks})…` : "Extracting audio…";
  return { label, fraction: Math.min(1, (p.track + p.fraction) / p.tracks) };
}
