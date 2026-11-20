// Settings → Resources: totals, the last minute of samples and the small graphs' shapes.

import type { ResourcePart, ResourceUsage } from "./api";

/** One sample is taken a second while the page is open; the graphs show this many. */
export const HISTORY = 60;

export type Metric = "ram" | "cpu" | "gpu" | "vram";

/** Klipt as a whole at one moment. GPU numbers are null when they can't be read. */
export type Totals = Record<Metric, number | null>;

export const PART_INFO: Record<ResourcePart, { label: string; detail: string }> = {
  app: { label: "Klipt", detail: "The app itself: library, uploads, game detection." },
  window: { label: "Window", detail: "Draws Klipt's screens. Stays loaded while Klipt sits in the tray." },
  recorder: { label: "Recorder", detail: "Records in the background and keeps the last moments in memory." },
  videoJobs: { label: "Video jobs", detail: "Exports, imports, screenshots and editor pictures, while they run." },
};

export function totals(usage: ResourceUsage): Totals {
  const sum = (f: (p: ResourceUsage["parts"][number]) => number | null) =>
    usage.gpuAvailable ? usage.parts.reduce((a, p) => a + (f(p) ?? 0), 0) : null;
  return {
    ram: usage.parts.reduce((a, p) => a + p.ramBytes, 0),
    cpu: usage.parts.reduce((a, p) => a + p.cpuPercent, 0),
    gpu: sum((p) => p.gpuPercent),
    vram: sum((p) => p.vramBytes),
  };
}

/** `history` with `sample` added at the end, the oldest dropped past `max`. */
export function pushSample<T>(history: T[], sample: T, max = HISTORY): T[] {
  const next = [...history, sample];
  return next.length > max ? next.slice(next.length - max) : next;
}

/**
 * The top of a graph's scale: a bit above the highest value so the line never touches the top,
 * but never under `floor` so near-idle noise stays a flat line along the bottom.
 */
export function scaleTop(values: (number | null)[], floor: number): number {
  const high = Math.max(0, ...values.map((v) => v ?? 0));
  return Math.max(floor, high * 1.25);
}

type Point = [x: number, y: number];

/**
 * Where each value sits in a `width`×`height` box: the newest at the right edge, room for `slots`
 * samples, so the line grows in from the right over the first minute. Split into runs at nulls.
 */
function runs(values: (number | null)[], top: number, width: number, height: number, slots: number): Point[][] {
  const step = width / Math.max(1, slots - 1);
  const offset = slots - values.length;
  const out: Point[][] = [];
  let run: Point[] | null = null;
  values.forEach((v, i) => {
    if (v == null) {
      run = null;
      return;
    }
    if (!run) out.push((run = []));
    run.push([round((offset + i) * step), round(height - (Math.min(v, top) / top) * height)]);
  });
  return out;
}

const line = (points: Point[]) => points.map(([x, y], i) => `${i ? "L" : "M"}${x} ${y}`).join("");

/** An SVG path for the graph's line. */
export function sparkPath(values: (number | null)[], top: number, width: number, height: number, slots = HISTORY): string {
  return runs(values, top, width, height, slots).map(line).join("");
}

/** The same line closed down to the bottom, for the soft fill under it. */
export function sparkArea(values: (number | null)[], top: number, width: number, height: number, slots = HISTORY): string {
  return runs(values, top, width, height, slots)
    .map((r) => `${line(r)}L${r.at(-1)![0]} ${height}L${r[0][0]} ${height}Z`)
    .join("");
}

const round = (n: number) => Math.round(n * 10) / 10;

/** "0%", "0.6%", "12%": one decimal under 10. */
export function formatPercent(p: number): string {
  if (p <= 0) return "0%";
  return p < 10 ? `${Math.max(p, 0.1).toFixed(1).replace(/\.0$/, "")}%` : `${Math.round(p)}%`;
}
