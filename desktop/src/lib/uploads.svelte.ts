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

/** Starts (or retries, resuming where it stopped) an upload. */
export async function startUpload(id: string, title: string, visibility: Visibility) {
  uploads[id] = { id, title, visibility, status: "starting", sent: 0, total: 0 };
  try {
    await api.uploadClip(id, title, visibility);
  } catch (e) {
    // "Already uploading" means the core is still on it: keep showing its progress.
    if (errorCode(e) === "already_uploading") {
      toast(errorMessage(e));
      return;
    }
    if (errorCode(e) === "unauthorized") {
      uploads[id] = { ...uploads[id], status: "needsLogin" };
      onUnauthorized?.();
      return;
    }
    uploads[id] = { ...uploads[id], status: "failed", message: errorMessage(e) };
  }
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
      const job = uploads[id];
      if (job) uploads[id] = { ...job, status: phase, sent, total };
    }),
    on("upload-done", ({ id, shareUrl, copied }) => {
      const job = uploads[id];
      if (job) uploads[id] = { ...job, status: "done", shareUrl, copied };
      handlers.done(shareUrl, copied);
    }),
    on("upload-failed", ({ id, code, message }) => {
      const job = uploads[id];
      if (!job) return;
      if (code === "unauthorized") {
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
