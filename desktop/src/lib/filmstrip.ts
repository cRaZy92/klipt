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
 * The tiles that cover a track `trackWidthPx` wide showing `view` of the clip (the whole clip by
 * default), each `tileWidthPx` wide. Tiles are fixed in time, so they slide when the view pans;
 * the last may run past the clip's end. `ms` is the time at the middle of the tile's part inside
 * the clip, `i` its number from the clip's start.
 */
export function tiles(
  trackWidthPx: number,
  tileWidthPx: number,
  durationMs: number,
  view: { from: number; to: number } = { from: 0, to: durationMs },
): { i: number; leftPct: number; widthPct: number; ms: number }[] {
  const span = view.to - view.from;
  if (!(trackWidthPx > 0) || !(tileWidthPx > 0) || !(span > 0)) return [];
  const tileMs = (tileWidthPx / trackWidthPx) * span;
  const first = Math.max(0, Math.floor(view.from / tileMs));
  // Rounding must not add a tile that starts at the end.
  const last = Math.ceil(Math.min(view.to, durationMs) / tileMs - 1e-9);
  const out = [];
  for (let i = first; i < last; i++) {
    const start = i * tileMs;
    const end = Math.min(start + tileMs, durationMs);
    out.push({
      i,
      leftPct: ((start - view.from) / span) * 100,
      widthPct: (tileWidthPx / trackWidthPx) * 100,
      ms: (start + end) / 2,
    });
  }
  return out;
}
