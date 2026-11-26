// The profile picture crop: a square of the picture (its centre and a zoom), shown round, then
// drawn at 512×512. Same maths as the website's crop dialog (server/public/app.js).

export interface Size {
  w: number;
  h: number;
}
export interface Point {
  x: number;
  y: number;
}

export const MIN_ZOOM = 1;
export const MAX_ZOOM = 4;

/** The square's side in picture pixels: the shorter side at zoom 1. */
const sideOf = (img: Size, zoom: number) => Math.min(img.w, img.h) / zoom;

/** `center` moved so the square stays inside the picture. */
function clampCenter(img: Size, zoom: number, center: Point): Point {
  const half = sideOf(img, zoom) / 2;
  return {
    x: Math.min(Math.max(center.x, half), img.w - half),
    y: Math.min(Math.max(center.y, half), img.h - half),
  };
}

/** The visible square in picture pixels, for `drawImage`. */
export function cropSquare(img: Size, zoom: number, center: Point): { sx: number; sy: number; size: number } {
  const size = sideOf(img, zoom);
  const c = clampCenter(img, zoom, center);
  return { sx: c.x - size / 2, sy: c.y - size / 2, size };
}

/** The centre after dragging by `dx`, `dy` view pixels in a view `viewPx` wide. */
export function panBy(img: Size, zoom: number, center: Point, dx: number, dy: number, viewPx: number): Point {
  const scale = sideOf(img, zoom) / viewPx;
  return clampCenter(img, zoom, { x: center.x - dx * scale, y: center.y - dy * scale });
}

/** A zoom kept within 1–4, with the centre still valid for it. */
export function zoomTo(img: Size, zoom: number, center: Point): { zoom: number; center: Point } {
  const z = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
  return { zoom: z, center: clampCenter(img, z, center) };
}
