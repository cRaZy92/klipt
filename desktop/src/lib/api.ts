// Typed access to the Rust core: Tauri commands and events. Errors from commands arrive as
// `{ code, message }`; the UI branches on `code` but only ever shows `message`.

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Visibility = "feed" | "link";
export type Sort = "newestFirst" | "oldestFirst";

export interface ClipView {
  id: string;
  path: string;
  createdAt: number;
  durationMs: number;
  width: number;
  height: number;
  hasMic: boolean;
  gameName: string | null;
  steamAppId: number | null;
  exe: string | null;
  title: string;
  trimStartMs: number | null;
  trimEndMs: number | null;
  gameVolume: number;
  micVolume: number;
  serverClipId: string | null;
  shareUrl: string | null;
  visibility: Visibility | null;
  uploadId: string | null;
  uploadOffset: number | null;
  fileMissing: boolean;
  thumbPath: string | null;
}

export interface GameCount {
  name: string;
  steamAppId: number | null;
  count: number;
}

export interface ListFilter {
  game?: string | null;
  sort?: Sort;
}

/** The fields the UI reads; the rest round-trips untouched through `setSettings`. */
export interface Settings {
  hotkey: string;
  bufferSeconds: number;
  outputDir: string;
  serverUrl: string | null;
  [key: string]: unknown;
}

export interface Account {
  serverUrl: string;
  username: string;
}

export interface HotkeyStatus {
  display: string | null;
  error: string | null;
}

export interface AppError {
  code: string;
  message: string;
}

export type UploadPhase = "exporting" | "uploading" | "finalizing";

export interface UploadProgress {
  id: string;
  sent: number;
  total: number;
  phase: UploadPhase;
}

export interface UploadDone {
  id: string;
  shareUrl: string;
  copied: boolean;
}

export interface UploadFailed {
  id: string;
  code: string;
  message: string;
  /** For `unauthorized`: the upload used an older login than the one saved now. */
  stale?: boolean;
}

const FALLBACK_MESSAGE = "Something went wrong. Please try again.";

function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && typeof (e as AppError).message === "string";
}

/** The friendly message of a command error. */
export function errorMessage(e: unknown): string {
  return isAppError(e) && e.message ? e.message : FALLBACK_MESSAGE;
}

export function errorCode(e: unknown): string | null {
  return isAppError(e) && typeof e.code === "string" ? e.code : null;
}

/** A local file as a URL the webview can load (thumbnails, and videos in the editor). */
export function fileUrl(path: string): string {
  return convertFileSrc(path);
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  getAccount: () => invoke<Account | null>("get_account"),
  checkServer: (serverUrl: string) => invoke<string>("check_server", { serverUrl }),
  login: (serverUrl: string, username: string, password: string) =>
    invoke<Account>("login", { serverUrl, username, password }),
  logout: () => invoke<void>("logout"),
  getHotkeyStatus: () => invoke<HotkeyStatus>("get_hotkey_status"),
  saveClipNow: () => invoke<void>("save_clip_now"),
  listClips: (filter: ListFilter) => invoke<ClipView[]>("list_clips", { filter }),
  listGames: () => invoke<GameCount[]>("list_games"),
  renameClip: (id: string, title: string) => invoke<void>("rename_clip", { id, title }),
  setClipGame: (id: string, name: string | null) => invoke<void>("set_clip_game", { id, name }),
  deleteClip: (id: string, deleteFile: boolean) => invoke<void>("delete_clip", { id, deleteFile }),
  showInFolder: (id: string) => invoke<void>("show_in_folder", { id }),
  copyShareLink: (id: string) => invoke<string>("copy_share_link", { id }),
  uploadClip: (id: string, title: string, visibility: Visibility) =>
    invoke<void>("upload_clip", { id, title, visibility }),
};

export interface Events {
  "clip-saved": ClipView;
  "save-busy": null;
  "recorder-error": AppError;
  "hotkey-bound": { display: string };
  "hotkey-error": AppError;
  "upload-progress": UploadProgress;
  "upload-done": UploadDone;
  "upload-failed": UploadFailed;
}

/**
 * Listens to a core event; returns a function that stops listening (usable as an `$effect`
 * cleanup even before Tauri finished registering the listener).
 */
export function on<K extends keyof Events>(event: K, handler: (payload: Events[K]) => void): () => void {
  let stopped = false;
  let unlisten: UnlistenFn | null = null;
  listen<Events[K]>(event, (e) => handler(e.payload))
    .then((fn) => {
      if (stopped) fn();
      else unlisten = fn;
    })
    .catch((e) => console.error(`can't listen to ${event}`, e));
  return () => {
    stopped = true;
    unlisten?.();
  };
}
