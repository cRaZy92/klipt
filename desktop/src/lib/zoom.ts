// Pure math for zooms in the editor. A zoom eases into its area over a short ramp, holds, and
// eases back out. The export (src-tauri/src/export.rs, `zoom_filter`) uses the same easing, so
// the preview and the exported file zoom at the same moments.

import type { ZoomRange } from "./api";
import type { TrimRange } from "./trim";
import { moveRange, resizeRange, roomAt } from "./ranges";

/** The least a zoom magnifies. */
export const MIN_ZOOM = 1.25;
/** The most a zoom magnifies: past this, 1080p footage turns to mush. */
export const MAX_ZOOM = 4;
/** The shortest a zoom can be. */
export const MIN_ZOOM_MS = 500;
/** How long a new zoom is (shorter when something follows sooner). */
export const DEFAULT_ZOOM_MS = 3000;
/** How far a new zoom magnifies. */
export const DEFAULT_ZOOM = 2;
/** The longest ease in (and out). */
export const ZOOM_RAMP_MS = 400;

/** What the picture shows at a moment: `scale`× around the point `x`, `y` (fractions). */
export interface ZoomView {
  scale: number;
  x: number;
  y: number;
}

/** A zoom's area as fractions of the frame. */
export interface BoxRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** How long zoom `z` eases in (and out): at most 400 ms, at most half its length. */
export function rampMs(z: ZoomRange): number {
  return clamp(Math.floor((z.endMs - z.startMs) / 2), 1, ZOOM_RAMP_MS);
}

/** How far into zoom `z` the picture is at `ms`: 0 (not zoomed) to 1 (fully), smoothed. */
export function easeAt(z: ZoomRange, ms: number): number {
  const r = rampMs(z);
  const u = clamp((ms - z.startMs) / r, 0, 1) * clamp((z.endMs - ms) / r, 0, 1);
  return u * u * (3 - 2 * u);
}

/** The index of the zoom playing at `ms`, or −1. */
export function zoomIndexAt(zooms: ZoomRange[], ms: number): number {
  return zooms.findIndex((z) => ms >= z.startMs && ms < z.endMs);
}

/** What the picture shows at `ms`. */
export function zoomAt(zooms: ZoomRange[], ms: number): ZoomView {
  const z = zooms[zoomIndexAt(zooms, ms)];
  if (!z) return { scale: 1, x: 0.5, y: 0.5 };
  return { scale: 1 + (z.zoom - 1) * easeAt(z, ms), x: z.x, y: z.y };
}

/**
 * A new zoom at `ms` (2×, centred, DEFAULT_ZOOM_MS long, shorter to fit before the next zoom or
 * the trim end), or null when there's no room for MIN_ZOOM_MS there.
 */
export function addZoomAt(zooms: ZoomRange[], ms: number, trim: TrimRange): ZoomRange[] | null {
  const room = roomAt(zooms, ms, trim, DEFAULT_ZOOM_MS, MIN_ZOOM_MS);
  if (!room) return null;
  const added: ZoomRange = { ...room, zoom: DEFAULT_ZOOM, x: 0.5, y: 0.5 };
  return [...zooms, added].sort((a, b) => a.startMs - b.startMs);
}

/** Zoom `i` moved to start at `startMs`, keeping its length, between its neighbours and the trim. */
export function moveZoom(zooms: ZoomRange[], i: number, startMs: number, trim: TrimRange): ZoomRange[] {
  return moveRange(zooms, i, startMs, trim);
}

/** Zoom `i` with its `edge` moved to `ms`: at least MIN_ZOOM_MS long, between its neighbours and the trim. */
export function resizeZoom(
  zooms: ZoomRange[],
  i: number,
  edge: "start" | "end",
  ms: number,
  trim: TrimRange,
): ZoomRange[] {
  return resizeRange(zooms, i, edge, ms, trim, MIN_ZOOM_MS);
}

/** Zoom `i` with a new area: the zoom kept in MIN_ZOOM..MAX_ZOOM, `x` and `y` in 0..1. */
export function setZoomBox(zooms: ZoomRange[], i: number, box: { zoom: number; x: number; y: number }): ZoomRange[] {
  if (!zooms[i]) return zooms;
  const zoom = clamp(box.zoom, MIN_ZOOM, MAX_ZOOM);
  const x = clamp(box.x, 0, 1);
  const y = clamp(box.y, 0, 1);
  return zooms.map((o, j) => (j === i ? { ...o, zoom, x, y } : o));
}

export function removeZoom(zooms: ZoomRange[], i: number): ZoomRange[] {
  return zooms.filter((_, j) => j !== i);
}

/** Zoom `z`'s area as fractions of the frame (it has the frame's aspect ratio). */
export function boxRect(z: { zoom: number; x: number; y: number }): BoxRect {
  const size = 1 / z.zoom;
  return { left: z.x * (1 - size), top: z.y * (1 - size), width: size, height: size };
}

/** The zoom and position of an area (the inverse of `boxRect`; the width sets the zoom). */
export function boxFromRect(rect: { left: number; top: number; width: number }): { zoom: number; x: number; y: number } {
  const zoom = clamp(1 / rect.width, MIN_ZOOM, MAX_ZOOM);
  const slack = 1 - 1 / zoom;
  return { zoom, x: clamp(rect.left / slack, 0, 1), y: clamp(rect.top / slack, 0, 1) };
}

/** Whether two zoom lists are the same. */
export function sameZooms(a: ZoomRange[], b: ZoomRange[]): boolean {
  return (
    a.length === b.length &&
    a.every(
      (z, i) =>
        z.startMs === b[i].startMs && z.endMs === b[i].endMs && z.zoom === b[i].zoom && z.x === b[i].x && z.y === b[i].y,
    )
  );
}
