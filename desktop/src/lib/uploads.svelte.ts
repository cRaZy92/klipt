// Uploads in progress, shared by the library cards and the upload dialog. The Rust core does the
// work; this mirrors its `upload-*` events.

import { api, errorCode, errorMessage, on, type UploadPhase, type Visibility } from "./api";
import { toast } from "./toasts.svelte";

export type UploadStatus = "starting" | UploadPhase | "done" | "failed" | "needsLogin";

export interface UploadJob {
  id: string;
  title: string;
  visibility: Visibility;
  status: UploadStatus;
  sent: number;
  total: number;
  /** Set when failed. */
  message?: string;
  shareUrl?: string;
  copied?: boolean;
}

export const uploads = $state<Record<string, UploadJob>>({});

/** True while the core is working on the clip. */
export function isBusy(job: UploadJob | undefined): boolean {
  return !!job && ["starting", "exporting", "uploading", "finalizing"].includes(job.status);
}

/** 0..1 for the current phase, or null when there's no measurable progress. */
export function fraction(job: UploadJob): number | null {
  if (job.status === "finalizing" || job.total <= 0) return null;
  return Math.min(1, Math.max(0, job.sent / job.total));
}

export function statusLabel(job: UploadJob): string {
  const pct = fraction(job);
  const percent = pct === null ? "" : ` ${Math.round(pct * 100)}%`;
  switch (job.status) {
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
    }
  }
}

/** Starts (or retries, resuming where it stopped) an upload. */
export async function startUpload(id: string, title: string, visibility: Visibility) {
  stoppedByLogout.delete(id);
  requested[id] = { title, visibility };
  const previous = uploads[id];
  try {
    await api.uploadClip(id, title, visibility);
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
}

let onUnauthorized: (() => void) | null = null;

/**
 * Mirrors the core's upload events. `unauthorized` is called when the login expired mid-upload:
 * the app shows the login screen, then calls `resumeAfterLogin`.
 */
export function watchUploads(handlers: { unauthorized: () => void; done: (shareUrl: string, copied: boolean) => void }) {
  onUnauthorized = handlers.unauthorized;
  const stops = [
    on("upload-progress", ({ id, sent, total, phase }) => {
      if (stoppedByLogout.has(id)) return;
      uploads[id] = { ...jobFor(id), status: phase, sent, total };
    }),
    on("upload-done", ({ id, shareUrl, copied }) => {
      // It finished before the logout reached the server: it's uploaded after all.
      stoppedByLogout.delete(id);
      const job = uploads[id];
      if (job) uploads[id] = { ...job, status: "done", shareUrl, copied };
      handlers.done(shareUrl, copied);
    }),
    on("upload-failed", ({ id, code, message, stale }) => {
      const job = uploads[id];
      if (!job) return;
      if (stoppedByLogout.has(id)) {
        // Already shown as stopped. A 401 here is the logout itself; anything else, show it.
        if (code !== "unauthorized") uploads[id] = { ...job, status: "failed", message };
        return;
      }
      if (code === "unauthorized" && stale) {
        // This upload used a login that was already replaced: retry with the current one.
        // With no current login, `upload_clip` refuses and that sends the user to log in.
        void startUpload(id, job.title, job.visibility);
      } else if (code === "unauthorized") {
        uploads[id] = { ...job, status: "needsLogin" };
        handlers.unauthorized();
      } else {
        uploads[id] = { ...job, status: "failed", message };
      }
    }),
  ];
  return () => {
    onUnauthorized = null;
    stops.forEach((stop) => stop());
  };
}
