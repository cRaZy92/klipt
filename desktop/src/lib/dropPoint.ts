/**
 * Where a file dragged over the window is, in CSS pixels. Tauri calls the drag position
 * physical, but only WebView2 (Windows) reports physical pixels; WebKitGTK (Linux) already
 * reports CSS pixels.
 */
export function dropPoint(pos: { x: number; y: number }, windows: boolean, pixelRatio: number): { x: number; y: number } {
  const scale = windows && pixelRatio > 0 ? pixelRatio : 1;
  return { x: pos.x / scale, y: pos.y / scale };
}

export const onWindows = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");
