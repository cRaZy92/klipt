// The Jobs list: background work the core keeps (uploads, imports, Combine, exports, downloads,
// moving the clips folder). This mirrors its `jobs-changed` events; the core has the list, so a
// window opened again after being released gets it back whole.

import { api, on, type Job, type JobKind } from "./api";
import { formatDuration, formatSize } from "./format";
import { formatRate, timeLeft } from "./progress";
import type { IconName } from "../components/Icon.svelte";

export const jobs = $state<{ list: Job[] }>({ list: [] });

export function watchJobs(): () => void {
  const stop = on("jobs-changed", (list) => (jobs.list = list));
  api
    .listJobs()
    .then((list) => (jobs.list = list))
    .catch(() => {});
  return stop;
}

export async function clearFinished() {
  jobs.list = await api.clearFinishedJobs();
}

export function runningCount(list: Job[]): number {
  return list.filter((j) => j.state === "running").length;
}

/** Running jobs first (oldest first, as they started), then finished ones, newest first. */
export function ordered(list: Job[]): Job[] {
  const running = list.filter((j) => j.state === "running").sort((a, b) => a.startedAtMs - b.startedAtMs);
  const finished = list
    .filter((j) => j.state !== "running")
    .sort((a, b) => (b.finishedAtMs ?? b.startedAtMs) - (a.finishedAtMs ?? a.startedAtMs));
  return [...running, ...finished];
}

const KINDS: Record<JobKind, string> = {
  upload: "Upload",
  import: "Import",
  combine: "Combine",
  export: "Export",
  download: "Download",
  move: "Move clips",
};

export function kindLabel(kind: JobKind): string {
  return KINDS[kind];
}

export const KIND_ICONS: Record<JobKind, IconName> = {
  upload: "upload",
  import: "folder",
  combine: "combine",
  export: "download",
  download: "download",
  move: "folder",
};

/** "62%": every job shows one, also between measured steps (the bar waits there). */
export function percent(job: Job): string {
  const p = Math.round(Math.min(1, Math.max(0, job.progress)) * 100);
  // Not "100%" while something is still left to do.
  return `${job.state === "running" ? Math.min(p, 99) : p}%`;
}

/** "24 MB of 38 MB · 3 MB/s · 5 s left", "29 of 40 · 2 min left", or null when there's nothing to add. */
export function detail(job: Job): string | null {
  const parts: string[] = [];
  const a = job.amount;
  if (a?.unit === "bytes" && a.total > 0) {
    parts.push(`${formatSize(a.done)} of ${formatSize(a.total)}`);
    if (job.state === "running" && a.rate && a.rate > 0) parts.push(formatRate(a.rate));
  } else if (a?.unit === "items" && a.total > 0) {
    parts.push(`${a.done} of ${a.total}`);
  }
  if (job.state === "running" && job.etaSecs !== null && job.etaSecs > 0) parts.push(timeLeft(job.etaSecs));
  return parts.length ? parts.join(" · ") : null;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** "14:02". */
export function clockTime(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** "Started 14:02 · 1:35 so far", or "14:02 → 14:05 · took 3:12" once it ended. */
export function timing(job: Job, now: number): string {
  const started = clockTime(job.startedAtMs);
  if (job.finishedAtMs === null) return `Started ${started} · ${formatDuration(Math.max(0, now - job.startedAtMs))} so far`;
  return `${started} → ${clockTime(job.finishedAtMs)} · took ${formatDuration(Math.max(0, job.finishedAtMs - job.startedAtMs))}`;
}
