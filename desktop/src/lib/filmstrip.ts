import { fileUrl } from "./api";

/** The filmstrip has one picture every this many ms. */
const FRAME_MS = 100;

/** The filmstrip picture nearest to `ms`. */
export function frameIndex(ms: number): number {
  return Math.max(0, Math.round(ms / FRAME_MS));
}

/**
 * The URL of picture `index` in the filmstrip folder `dir`. A Windows folder (backslashes only)
 * is joined with a backslash, anything else with "/". `toUrl` turns the path into a URL the
 * page can load (tests pass the path through, as convertFileSrc only works inside Tauri).
 */
export function frameUrl(dir: string, index: number, toUrl: (path: string) => string = fileUrl): string {
  const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return toUrl(`${dir}${sep}${String(index).padStart(6, "0")}.jpg`);
}

/** The picture to show for `ms` while `ready` pictures exist, or null while it isn't made yet. */
export function previewIndex(ms: number, ready: number): number | null {
  const i = frameIndex(ms);
  return i < ready ? i : null;
}

/**
 * The tiles that cover a track `trackWidthPx` wide, each `tileWidthPx` wide (the last may run past
 * the end). `ms` is the time at the middle of the part of the tile that is on the track.
 */
export function tiles(
  trackWidthPx: number,
  tileWidthPx: number,
  durationMs: number,
): { leftPct: number; widthPct: number; ms: number }[] {
  if (!(trackWidthPx > 0) || !(tileWidthPx > 0)) return [];
  const count = Math.ceil(trackWidthPx / tileWidthPx);
  return Array.from({ length: count }, (_, i) => {
    const left = i * tileWidthPx;
    const visibleMiddle = (left + Math.min(left + tileWidthPx, trackWidthPx)) / 2;
    return {
      leftPct: (left / trackWidthPx) * 100,
      widthPct: (tileWidthPx / trackWidthPx) * 100,
      ms: (visibleMiddle / trackWidthPx) * durationMs,
    };
  });
}
