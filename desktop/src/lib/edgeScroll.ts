// Auto-scroll for drags on a zoomed-in timeline: held near or past a lane's edge, the view keeps
// moving that way, so a handle or block can be dragged further than the screen shows.

/** How close to an edge (px) the pointer starts the scroll. */
export const EDGE_ZONE_PX = 32;
/** The fastest it goes, in screen widths per second (reached three zones past the edge). */
const MAX_SPEED = 2;

/**
 * How fast the view moves for a pointer `x` px into an area `width` px wide, in screen widths per
 * second: 0 away from the edges, growing from the zone's inner side to the edge (0.5) and past it;
 * negative towards the left.
 */
export function edgeSpeed(x: number, width: number, zone = EDGE_ZONE_PX): number {
  if (!(width > 0) || !Number.isFinite(x) || !(zone > 0)) return 0;
  // On a narrow area the zones would meet: keep them to a quarter of it each.
  const z = Math.min(zone, width / 4);
  const depth = x < z ? z - x : x > width - z ? x - (width - z) : 0;
  if (depth <= 0) return 0;
  const speed = Math.min(MAX_SPEED, (depth / z) * 0.5);
  return x < z ? -speed : speed;
}

/** A loop that calls `frame` with the ms since the last frame, once per animation frame, while started. */
export function edgeScroller(frame: (dtMs: number) => void): { start: () => void; stop: () => void } {
  let raf = 0;
  let last = 0;
  function loop(t: number) {
    // The first frame and a stalled one count as one ordinary frame.
    const dt = last ? Math.min(50, t - last) : 16;
    last = t;
    frame(dt);
    raf = requestAnimationFrame(loop);
  }
  return {
    start() {
      if (raf) return;
      last = 0;
      raf = requestAnimationFrame(loop);
    },
    stop() {
      cancelAnimationFrame(raf);
      raf = 0;
    },
  };
}
