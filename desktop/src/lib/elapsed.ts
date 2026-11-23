import { formatSize } from "./format";

/** How long something has run, as a clock: "0:07", "12:34", "1:02:03". */
export function formatElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** A running recording's button text: "12:04 · 3.1 GB", or just the time until its size is known. */
export function recordingLabel(elapsedMs: number, sizeBytes: number | null): string {
  const time = formatElapsed(elapsedMs);
  return sizeBytes === null ? time : `${time} · ${formatSize(sizeBytes)}`;
}
