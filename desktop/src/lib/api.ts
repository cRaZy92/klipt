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

export type RecordingMode = "always" | "whileGameRunning";
export type BufferLocation = "ram" | "disk";
export type Resolution = "p720" | "p1080" | "p1440" | "native";
export type Quality = "low" | "medium" | "high" | "custom";

/** Mirrors `settings.rs`. */
export interface Settings {
  recordingMode: RecordingMode;
  bufferLocation: BufferLocation;
  /** Null = automatic (the primary display). */
  displayId: string | null;
  resolution: Resolution;
  fps: number;
  quality: Quality;
  customBitrateKbps: number;
  bufferSeconds: number;
  micEnabled: boolean;
  /** Null = the default microphone. */
  micDeviceId: string | null;
  /** Canonical accelerator, e.g. "Alt+Backquote". */
  hotkey: string;
  outputDir: string;
  startWithOs: boolean;
  saveSound: boolean;
  saveNotification: boolean;
  serverUrl: string | null;
}

export interface FieldError {
  field: string;
  message: string;
}

/** What `set_settings` returns: the saved settings and which side effects ran. */
export interface SettingsSaved {
  settings: Settings;
  /** The recorder restarted (its buffer was cleared). */
  recorderRestarted: boolean;
  /** The save-clip shortcut was bound again; `hotkeyDisplay` is what the system reports. */
  hotkeyDisplay: string | null;
}

export interface Display {
  id: string;
  name: string;
  width: number;
  height: number;
  primary: boolean;
}

export interface AudioDevice {
  id: string;
  name: string;
}

export interface Account {
  serverUrl: string;
  username: string;
}

export interface HotkeyStatus {
  display: string | null;
  error: string | null;
  /** `hotkey_taken` or `hotkey_unavailable` when `error` is set. */
  errorCode: string | null;
}

export interface AppError {
  code: string;
  message: string;
  /** Per-field problems, for `invalid_settings`. */
  fields?: FieldError[];
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

/** What `open_editor` returns: the clip plus the files the editor plays. */
export interface EditorData {
  clip: ClipView;
  videoPath: string;
  gameAudioPath: string;
  /** Null when the clip has no mic track. */
  micAudioPath: string | null;
  videoUrl: string;
  gameAudioUrl: string;
  micAudioUrl: string | null;
  durationMs: number;
}

/** A clip's edit. A null trim end means "to the end"; a null start, "from the start". */
export interface EditInput {
  trimStartMs: number | null;
  trimEndMs: number | null;
  /** 0.0–1.0 */
  gameVolume: number;
  /** 0.0–1.0 */
  micVolume: number;
}

export interface ExportProgress {
  id: string;
  /** 0..1 */
  progress: number;
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

/** The per-field messages of an `invalid_settings` error. */
export function errorFields(e: unknown): FieldError[] {
  return isAppError(e) && Array.isArray(e.fields) ? e.fields : [];
}

/** A local file as a URL the webview can load (thumbnails, and videos in the editor). */
export function fileUrl(path: string): string {
  return convertFileSrc(path);
}

/** A clip on the server's shared feed (the server's `Clip`). */
export interface FeedClip {
  id: string;
  title: string;
  gameName: string | null;
  steamAppId: number | null;
  durationMs: number;
  width: number;
  height: number;
  createdAt: string;
  owner: { username: string };
  shareUrl: string;
  videoUrl: string;
  thumbnailUrl: string;
}

export interface FeedPage {
  items: FeedClip[];
  nextCursor: string | null;
}

export interface FeedGame {
  gameName: string;
  steamAppId: number | null;
  count: number;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  /** Saves and applies settings: restarts the recorder, rebinds the shortcut, toggles autostart. */
  setSettings: (settings: Settings) => invoke<SettingsSaved>("set_settings", { settings }),
  listDisplays: () => invoke<Display[]>("list_displays"),
  /** Microphones (inputs only). */
  listAudioDevices: () => invoke<AudioDevice[]>("list_audio_devices"),
  openClipsFolder: () => invoke<void>("open_clips_folder"),
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
  feedClips: (game: string | null, cursor: string | null) => invoke<FeedPage>("feed_clips", { game, cursor }),
  feedGames: () => invoke<FeedGame[]>("feed_games"),
  copyText: (text: string) => invoke<void>("copy_text_to_clipboard", { text }),
  /** Opens the Klipt website in the browser; `path` like `/me`. */
  openWebsite: (path?: string) => invoke<void>("open_website", { path: path ?? null }),
  openEditor: (id: string) => invoke<EditorData>("open_editor", { id }),
  saveEdit: (id: string, edit: EditInput) => invoke<void>("save_edit", { id, ...edit }),
  /** Resolves once the copy is written; progress arrives as `export-progress`. */
  exportCopy: (id: string, destPath: string, edit: EditInput) =>
    invoke<void>("export_copy", { id, destPath, ...edit }),
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
  "export-progress": ExportProgress;
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
