/** A time as `m:ss.t` (tenths of a second, rounded down); negative or invalid shows as zero. */
export function formatTenths(ms: number): string {
  const tenths = Number.isFinite(ms) && ms > 0 ? Math.floor(ms / 100) : 0;
  const seconds = Math.floor(tenths / 10);
  const m = Math.floor(seconds / 60);
  const s = String(seconds % 60).padStart(2, "0");
  return `${m}:${s}.${tenths % 10}`;
}

/** One frame in ms at `fps` (0 = unknown: 60 fps is assumed). */
export function stepMs(fps: number): number {
  return 1000 / (fps > 0 ? fps : 60);
}
