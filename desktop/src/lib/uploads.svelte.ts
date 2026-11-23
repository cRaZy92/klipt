// Uploads in progress, shared by the library cards and the upload dialog. The Rust core does the
// work; this mirrors its `upload-*` events.

import { api, errorCode, errorMessage, on, type UploadPhase, type UploadToast, type Visibility } from "./api";
import { RateMeter, transferDetail } from "./progress";
import { toast } from "./toasts.svelte";

export type UploadStatus = "queued" | "starting" | UploadPhase | "done" | "failed" | "needsLogin";

export interface UploadJob {
  id: string;
  title: string;
  visibility: Visibility;
  status: UploadStatus;
  sent: number;
  total: number;
  /** Bytes per second while uploading, once known. */
  rate?: number | null;
  /** Set when failed. */
  message?: string;
  shareUrl?: string;
  copied?: boolean;
  /** Done by replacing the video at the existing link. */
  replaced?: boolean;
}

export const uploads = $state<Record<string, UploadJob>>({});

/** True while the core is working on the clip. */
export function isBusy(job: UploadJob | undefined): boolean {
  return !!job && ["queued", "starting", "exporting", "uploading", "finalizing"].includes(job.status);
}

/** 0..1 for the current phase, or null when there's no measurable progress. */
export function fraction(job: UploadJob): number | null {
  if (job.status === "finalizing" || job.status === "queued" || job.total <= 0) return null;
  return Math.min(1, Math.max(0, job.sent / job.total));
}

/** "12 MB of 38 MB · 2 MB/s · 13 s left" while uploading, else null. */
export function transferLabel(job: UploadJob): string | null {
  if (job.status !== "uploading" || job.total <= 0) return null;
  return transferDetail(job.sent, job.total, job.rate ?? null);
}

export function statusLabel(job: UploadJob): string {
  const pct = fraction(job);
  const percent = pct === null ? "" : ` ${Math.round(pct * 100)}%`;
  switch (job.status) {
    case "queued":
      return "Waiting to upload…";
    case "starting":
      return "Starting…";
    case "exporting":
      return `Preparing video…${percent}`;
    case "uploading":
      return `Uploading…${percent}`;
    case "finalizing":
      return "Finishing up…";
    case "done":
      return "Uploaded";
    case "needsLogin":
      return "Waiting for you to log in";
    case "failed":
      return job.message ?? "Upload failed.";
  }
}

/** What each clip was last asked to upload as, so events for it can always be shown. */
const requested: Record<string, { title: string; visibility: Visibility }> = {};

/**
 * Uploads stopped because the user logged out. The core only notices at its next request (a 401),
 * so events for these are ignored until the user starts the upload again.
 */
const stoppedByLogout = new Set<string>();

export const LOGGED_OUT_MESSAGE = "Stopped because you logged out. Try again to upload it.";

/**
 * Called when the user logs out: unfinished uploads stop and wait for "Try again" instead of
 * reporting an expired session and carrying on under whoever logs in next.
 */
export function stopUploadsForLogout() {
  for (const job of Object.values(uploads)) {
    if (isBusy(job) || job.status === "needsLogin") {
      stoppedByLogout.add(job.id);
      uploads[job.id] = { ...job, status: "failed", message: LOGGED_OUT_MESSAGE };
      settle(job.id);
    }
  }
}

/** Waiters for uploads to end (done, failed or waiting for a login), by clip id. */
const waiters = new Map<string, () => void>();

function settle(id: string) {
  waiters.get(id)?.();
  waiters.delete(id);
}

/** Resolves once the clip's upload is no longer running. */
function settled(id: string): Promise<void> {
  return new Promise((resolve) => {
    if (!isBusy(uploads[id]) || uploads[id].status === "queued") resolve();
    else waiters.set(id, resolve);
  });
}


/**
 * Uploads several clips one after another (each exports with ffmpeg first, so not all at once).
 * They show "Waiting to upload…" until their turn. Returns how many were uploaded.
 */
export async function uploadInTurn(list: { id: string; title: string }[], visibility: Visibility): Promise<number> {
  const todo = list.filter(({ id }) => !isBusy(uploads[id]));
  for (const { id, title } of todo) {
    uploads[id] = { id, title, visibility, status: "queued", sent: 0, total: 0 };
  }
  let uploaded = 0;
  for (const [index, { id, title }] of todo.entries()) {
    // Stopped meanwhile (logout) or waiting for a login: its turn is skipped.
    if (uploads[id]?.status !== "queued") continue;
    // More to come: the core holds its "uploaded" notice for one summary at the end.
    const more = todo.slice(index + 1).some((rest) => uploads[rest.id]?.status === "queued");
    await startUpload(id, title, visibility, undefined, undefined, more);
    // "Already uploading": the core is on it after all, and its events will say when it ends.
    if (uploads[id]?.status === "queued") uploads[id] = { ...uploads[id], status: "starting" };
    await settled(id);
    const status = uploads[id]?.status;
    if (status === "done") uploaded++;
    if (status === "needsLogin") {
      // The rest wait for the login too; logging in resumes them.
      for (const rest of todo) {
        if (uploads[rest.id]?.status === "queued") uploads[rest.id] = { ...uploads[rest.id], status: "needsLogin" };
      }
    }
  }
  return uploaded;
}

/** Friends tagged in each upload, kept for retries. */
const taggedFor: Record<string, string[]> = {};
/** Uploads that replace the video at the clip's link, kept for retries. */
const replaceFor: Record<string, boolean> = {};

/**
 * Starts (or retries, resuming where it stopped) an upload. `tagged` and `replace` are kept for
 * retries.
 */
export async function startUpload(
  id: string,
  title: string,
  visibility: Visibility,
  tagged?: string[],
  replace?: boolean,
  more = false,
) {
  stoppedByLogout.delete(id);
  requested[id] = { title, visibility };
  if (tagged) taggedFor[id] = tagged;
  if (replace !== undefined) replaceFor[id] = replace;
  const previous = uploads[id];
  try {
    await api.uploadClip(id, title, visibility, taggedFor[id] ?? [], replaceFor[id] ?? false, more);
  } catch (e) {
    // "Already uploading" means the core is still on it: leave what's shown alone; its
    // progress events keep updating the card.
    if (errorCode(e) === "already_uploading") {
      toast(errorMessage(e));
      return;
    }
    if (errorCode(e) === "unauthorized") {
      uploads[id] = { id, title, visibility, status: "needsLogin", sent: 0, total: 0 };
      onUnauthorized?.();
      return;
    }
    uploads[id] = { id, title, visibility, status: "failed", sent: 0, total: 0, message: errorMessage(e) };
    return;
  }
  // Started. Unless a progress event already arrived, show "Starting…" until one does.
  if (uploads[id] === previous) uploads[id] = { id, title, visibility, status: "starting", sent: 0, total: 0 };
}

/** The job for an event, created if this window didn't start it (e.g. it was reloaded). */
function jobFor(id: string): UploadJob {
  const known = requested[id] ?? { title: "", visibility: "feed" as Visibility };
  return uploads[id] ?? { id, ...known, status: "starting", sent: 0, total: 0 };
}

export function retryUpload(id: string) {
  const job = uploads[id];
  if (job) void startUpload(id, job.title, job.visibility);
}

export function forgetUpload(id: string) {
  delete uploads[id];
}

/** Restarts every upload that stopped because the login expired. Called after logging in. */
export function resumeAfterLogin() {
  for (const job of Object.values(uploads)) {
    if (job.status === "needsLogin") void startUpload(job.id, job.title, job.visibility);
  }
  // Uploads left over from an earlier run that waited for the login.
  void api.resumePendingUploads().catch(() => {});
}

/**
 * Shows the uploads left unfinished by an earlier run (the core carries them on by itself) as
 * waiting cards, with their titles. Called at startup.
 */
export async function loadPendingUploads() {
  let rows;
  try {
    rows = await api.pendingUploads();
  } catch {
    return;
  }
  for (const row of rows) {
    requested[row.clipId] = { title: row.title, visibility: row.visibility };
    taggedFor[row.clipId] = row.tagged;
    replaceFor[row.clipId] = row.replace;
    // An event that arrived meanwhile is newer.
    if (!uploads[row.clipId]) {
      uploads[row.clipId] = { id: row.clipId, title: row.title, visibility: row.visibility, status: "queued", sent: 0, total: 0 };
    }
  }
}

let onUnauthorized: (() => void) | null = null;

/** Upload speed per clip id, while it's uploading. */
const meters = new Map<string, RateMeter>();

/**
 * Mirrors the core's upload events. `unauthorized` is called when the login expired mid-upload:
 * the app shows the login screen, then calls `resumeAfterLogin`.
 */
export function watchUploads(handlers: {
  unauthorized: () => void;
  /** The core asks the window (it's in front) to show this notice. */
  notice: (toast: UploadToast) => void;
}) {
  onUnauthorized = handlers.unauthorized;
  const stops = [
    on("upload-progress", ({ id, sent, total, phase }) => {
      if (stoppedByLogout.has(id)) return;
      let rate: number | null = null;
      if (phase === "uploading") {
        const meter = meters.get(id) ?? new RateMeter();
        meters.set(id, meter);
        meter.add(performance.now(), sent);
        rate = meter.rate();
      } else {
        meters.delete(id);
      }
      uploads[id] = { ...jobFor(id), status: phase, sent, total, rate };
    }),
    on("upload-summary", (toast) => handlers.notice(toast)),
    on("upload-done", ({ id, shareUrl, copied, replaced, toast }) => {
      meters.delete(id);
      // It finished before the logout reached the server: it's uploaded after all.
      stoppedByLogout.delete(id);
      const job = uploads[id];
      if (job) uploads[id] = { ...job, status: "done", shareUrl, copied, replaced };
      if (toast) handlers.notice(toast);
      settle(id);
    }),
    on("upload-failed", ({ id, code, message, stale }) => {
      meters.delete(id);
      const job = uploads[id];
      if (!job) return;
      if (stoppedByLogout.has(id)) {
        // Already shown as stopped. A 401 here is the logout itself; anything else, show it.
        if (code !== "unauthorized") uploads[id] = { ...job, status: "failed", message };
        settle(id);
        return;
      }
      if (code === "unauthorized" && stale) {
        // This upload used a login that was already replaced: retry with the current one.
        // With no current login, `upload_clip` refuses and that sends the user to log in.
        void startUpload(id, job.title, job.visibility);
      } else if (code === "unauthorized") {
        uploads[id] = { ...job, status: "needsLogin" };
        handlers.unauthorized();
        settle(id);
      } else {
        uploads[id] = { ...job, status: "failed", message };
        settle(id);
      }
    }),
  ];
  return () => {
    onUnauthorized = null;
    stops.forEach((stop) => stop());
  };
}
