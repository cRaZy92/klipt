// Typed access to the Rust core: Tauri commands and events. Errors from commands arrive as
// `{ code, message }`; the UI branches on `code` but only ever shows `message`.

import type { Looks, MyAchievements } from "@klipt/shared";
import type { EditorProgress } from "./progress";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Where a clip is listed; its link works for anyone in all three. */
export type Visibility = "link" | "friends" | "public";

/** An upload started but not finished; it carries on after a restart. */
export interface PendingUpload {
  clipId: string;
  title: string;
  visibility: Visibility;
  tagged: string[];
  replace: boolean;
  queuedAtMs: number;
  /** Server albums it goes into once it's online, by id. */
  albums: string[];
  /** What the core does with it: running, queued for its resume pass, waiting for a login, or
   * stopped and tried again in a while. */
  status: "running" | "queued" | "needsLogin" | "waiting";
}
export type Sort = "newestFirst" | "oldestFirst";

/** A part removed in the editor, in ms into the video file. */
export interface CutRange {
  startMs: number;
  endMs: number;
}

/**
 * A zoom set in the editor: during startMs..endMs (ms into the video file) the picture eases into
 * `zoom`× and back out. `x` and `y` (0–1) place the zoomed area within the slack.
 */
/** A subtitle typed in the editor: `text` (one line) shows during `startMs..endMs` (ms into the file). */
export interface SubtitleRange {
  startMs: number;
  endMs: number;
  text: string;
}

/**
 * A part of one audio track muted or turned down in the editor: during startMs..endMs (ms into the
 * file) track `track` (0 = the game, then mic, voice chat and any others, as the file has them)
 * plays at `level` (0 = silent, below 1) of its volume. The video isn't cut.
 */
export interface AudioRange {
  track: number;
  startMs: number;
  endMs: number;
  level: number;
}

export interface ZoomRange {
  startMs: number;
  endMs: number;
  zoom: number;
  x: number;
  y: number;
}

/** A part of the clip whose whole picture uploads and exports blur (ms into the file). */
export interface BlurRange {
  startMs: number;
  endMs: number;
}

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
  /** Parts removed in the editor, sorted, inside the trim. */
  removedRanges: CutRange[];
  /** Zooms set in the editor, sorted, never overlapping. Missing = none. */
  zoomRanges?: ZoomRange[];
  /** Blurred parts set in the editor, sorted, never overlapping. Missing = none. */
  blurRanges?: BlurRange[];
  subtitles?: SubtitleRange[];
  /** Muted and turned-down parts of the audio tracks, sorted by track, then time. Missing = none. */
  audioRanges?: AudioRange[];
  gameVolume: number;
  micVolume: number;
  serverClipId: string | null;
  shareUrl: string | null;
  visibility: Visibility | null;
  uploadId: string | null;
  uploadOffset: number | null;
  /** A video clip or a screenshot (PNG). */
  kind: "clip" | "screenshot";
  /** A full recording (started and stopped by the user), not a replay-buffer clip. */
  isRecording: boolean;
  /** Starred; the automatic cleanup never deletes it. */
  favorite: boolean;
  /** Voice chat (Discord…) has its own track. */
  hasVoice: boolean;
  voiceVolume: number;
  /** The whole clip's volume, applied after mixing. */
  masterVolume: number;
  /** Volumes of other audio tracks (an imported clip's), by track index; missing = 1. */
  extraVolumes?: Record<string, number>;
  /** Uploads and exports blur the picture (only the sound matters). Missing = off. */
  blurVideo?: boolean;
  /** The thumbnail's frame (ms into the video); null = automatic. */
  thumbnailMs: number | null;
  fileMissing: boolean;
  thumbPath: string | null;
}

/** How many clips an album holds at most (`MAX_ALBUM_CLIPS` in packages/shared). */
export const MAX_ALBUM_CLIPS = 200;

/** An album on the server (`AlbumSummary` in packages/shared): clips under one link. */
export interface AlbumSummary {
  id: string;
  title: string;
  clipCount: number;
  coverUrl: string | null;
  shareUrl: string;
  updatedAt: string;
  /** Asked for one clip: whether it's in the album. */
  hasClip?: boolean;
}

/** An album with its clips, in order (`Album` in packages/shared). */
export interface Album extends AlbumSummary {
  owner: Person;
  clips: FeedClip[];
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
  /** The take-screenshot shortcut; "" = none. */
  screenshotHotkey: string;
  /** The start/stop recording shortcut; "" = none. */
  recordHotkey: string;
  outputDir: string;
  startWithOs: boolean;
  saveSound: boolean;
  saveNotification: boolean;
  serverUrl: string | null;
  /** Delete clips older than `cleanupDays` from this PC, of the kinds checked below. A clip is
   * deleted only if every kind it has is checked (an uploaded favorite needs both). */
  cleanupEnabled: boolean;
  cleanupDays: number;
  /** The cleanup may delete clips that were never uploaded. */
  cleanupLocal: boolean;
  /** The cleanup may delete uploaded clips (the server keeps its copy). */
  cleanupUploaded: boolean;
  /** The cleanup may delete favorites. */
  cleanupFavorites: boolean;
  /** Disk the editor's scrubbing pictures may take, in GB; the least recently opened are deleted first. */
  editorCacheGb: number;
  /** A system notification when a friend shares a clip. */
  friendNotifications: boolean;
  /** Copy the share link as soon as an upload finishes (else a Copy link button). */
  copyLinkOnUpload: boolean;
  /** Voice chat apps recorded on their own track (Linux); empty = recorded with the game. */
  voiceApps: string[];
  /** The first-start setup was finished or skipped. */
  onboarded: boolean;
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
  screenshotHotkeyDisplay: string | null;
  screenshotHotkeyError: string | null;
  recordHotkeyDisplay: string | null;
  recordHotkeyError: string | null;
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

/** Someone on the server as lists show them; picture and frame are missing on older servers. */
export interface Person {
  username: string;
  displayName?: string;
  avatarUrl?: string | null;
  frame?: string | null;
}

/** Where you stand with someone. */
export type FriendState = "none" | "outgoing" | "incoming" | "friends" | "self";

/** Someone's profile head (the server's `UserProfile`). A Friends profile seen by someone who
 * isn't a friend is `full: false`: no banner, stats or badges. */
export interface UserProfile {
  user: Person & { displayName: string };
  full: boolean;
  banner: string | null;
  stats: { clips: number; views: number; top: { id: string; title: string; views: number } | null } | null;
  badges: { id: string; name: string; description: string; icon: string }[];
  friendState: FriendState | null;
  /** Their profile page on the website. */
  profileUrl: string;
  /** When they made their account (ISO); missing from older servers. */
  joinedAt?: string;
}

/** `listFriends`: your friends and the requests either way. */
export interface FriendList {
  friends: Person[];
  /** People asking to be your friend. */
  incoming: Person[];
  /** People you asked. */
  outgoing: Person[];
}

/** A referral link: works once, for 14 days. */
export interface Referral {
  code: string;
  url: string;
  expiresAt: string;
}

export interface Account {
  serverUrl: string;
  username: string;
  /** The name friends see: the display name, or the username without one. */
  displayName: string;
  /** The display name as set; null when none is. */
  customName: string | null;
  /** A server admin: may upload in high quality. */
  isAdmin: boolean;
  /** Your picture; null draws the initial. */
  avatarUrl: string | null;
  /** Equipped accent, theme, frame and banner; null before the server sent any. */
  looks: Looks | null;
  /** Who sees your profile in full; the default for new uploads. */
  profileVisibility: "friends" | "public";
}

/** A line of what a problem report says about this PC. */
export interface InfoLine {
  label: string;
  value: string;
}

/** A running app that has its own shortcuts, and which of Klipt's it shares. */
export interface ShortcutApp {
  name: string;
  shortcuts: string[];
  clashes: string[];
}

export type FeedbackKind = "bug" | "idea";

/** The name to show for a person: their display name (newer servers) or username. */
export function nameOf(p: { username: string; displayName?: string | null }): string {
  return p.displayName || p.username;
}

export interface HotkeyStatus {
  display: string | null;
  error: string | null;
  /** `hotkey_taken` or `hotkey_unavailable` when `error` is set. */
  errorCode: string | null;
  /** The screenshot shortcut as the system reports it. */
  screenshotDisplay?: string | null;
  /** Why the screenshot shortcut isn't bound (saving clips still works). */
  screenshotError?: string | null;
  /** The start/stop recording shortcut as the system reports it. */
  recordDisplay?: string | null;
  /** Why the recording shortcut isn't bound (saving clips still works). */
  recordError?: string | null;
}

export interface AppError {
  code: string;
  message: string;
  /** Per-field problems, for `invalid_settings`. */
  fields?: FieldError[];
}

/** `preparing`: uploaded, waiting for the server to make the clip's link ready to share. */
export type UploadPhase = "exporting" | "uploading" | "finalizing" | "preparing";

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
  /** The video at the clip's existing link was replaced (the link is the same). */
  replaced: boolean;
  /** The notice for the window to show (it's in front); null: a system notification or nothing. */
  toast: UploadToast | null;
}

/** "Clip uploaded" with a Copy link button (`link` empty: no button). */
export interface UploadToast {
  title: string;
  link: string;
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
  /** Null when the clip has no voice chat track. */
  voiceAudioPath: string | null;
  videoUrl: string;
  gameAudioUrl: string;
  micAudioUrl: string | null;
  voiceAudioUrl: string | null;
  /** The clip's other audio tracks (an imported clip's), each with its own slider. */
  extraTracks: { index: number; audioPath: string; audioUrl: string }[];
  durationMs: number;
  /** The clip's frame rate; 0 when unknown. */
  fps: number;
  /** JPEG frames (`000000.jpg`, one per 100 ms) in `dir`; `frames` are readable now, the rest follow. */
  filmstrip: { dir: string; frames: number; total: number };
  /** A picture is the thumbnail, not a frame. */
  thumbImage: boolean;
}

/** A clip's edit. A null trim end means "to the end"; a null start, "from the start". */
export interface EditInput {
  trimStartMs: number | null;
  trimEndMs: number | null;
  /** 0.0–1.0 */
  gameVolume: number;
  /** 0.0–1.0 */
  micVolume: number;
  /** 0.0–1.0; voice chat (Linux clips that have it). */
  voiceVolume: number;
  /** 0.0–1.0; the whole mix. */
  masterVolume: number;
  /** 0.0–1.0 per other audio track, by track index. */
  extraVolumes: Record<number, number>;
  /** Blur the picture in uploads and exports. */
  blurVideo: boolean;
  /** Parts removed inside the trim: uploads and exports join the rest. */
  removedRanges: CutRange[];
  /** Zooms: uploads and exports ease into each and back out. */
  zoomRanges: ZoomRange[];
  /** Blurred parts: uploads and exports blur the whole picture there. */
  blurRanges: BlurRange[];
  subtitles: SubtitleRange[];
  /** Muted and turned-down parts of the audio tracks: uploads and exports turn them down too. */
  audioRanges: AudioRange[];
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

export type JobKind = "upload" | "import" | "combine" | "export" | "download" | "move";
export type JobState = "running" | "done" | "failed" | "cancelled";

/** How much of a job's current step is done. */
export type JobAmount =
  | { unit: "bytes"; done: number; total: number; rate: number | null }
  | { unit: "items"; done: number; total: number };

/** Background work, as the core's Jobs list keeps it (`jobs::JobView`). */
export interface Job {
  id: number;
  kind: JobKind;
  title: string;
  clipId: string | null;
  state: JobState;
  /** What it's doing now ("Uploading"), or how it ended. */
  step: string;
  /** The whole job, 0..1. */
  progress: number;
  etaSecs: number | null;
  amount: JobAmount | null;
  /** A failed job's error, or a short result. */
  message: string | null;
  startedAtMs: number;
  finishedAtMs: number | null;
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
  owner: Person;
  shareUrl: string;
  videoUrl: string;
  thumbnailUrl: string;
  /** Friends who watched it (each once, never the owner) plus logged-out plays. */
  views: number;
  /** Friends in the clip, by username. Missing from older servers. */
  tagged?: string[];
  /** Their display names, in the same order (newer servers). */
  taggedNames?: string[];
  /** A video clip or a screenshot; missing from older servers (a clip). For a screenshot
   * `videoUrl` is the picture. */
  kind?: "clip" | "screenshot";
  /** You starred it (on the server, so it follows you everywhere). Missing from older servers. */
  favorite?: boolean;
  /** The server is still compressing it (it plays meanwhile): waiting its turn, or being
   * compressed. Null when done; missing from older servers. */
  processing?: "queued" | "running" | null;
  /** False while the server is making a long clip's Discord embed: the link isn't handed out yet.
   * Missing from older servers (ready). */
  shareReady?: boolean;
}

/** What "Check for updates" found. On Linux `command` installs it (run in a terminal). */
export type UpdateCheck =
  | { status: "available"; version: string; command: string | null }
  | { status: "upToDate" }
  | { status: "unsupported"; reason: string };

/** How an import from a folder went (`import-finished`). */
export interface ImportSummary {
  imported: number;
  /** Of `imported`, how many got an H.264 copy (the originals were H.265/AV1, MKV…). */
  converted: number;
  alreadyThere: number;
  skipped: { file: string; reason: string }[];
  cancelled: boolean;
  elapsedSecs: number;
}

/** What importing a checked folder will do, for the confirmation. */
export interface ImportPreview {
  folder: string;
  files: number;
  /** Imported as they are: instant. */
  inPlace: number;
  /** Copied as they are into the clips folder (a drop with "copy"). */
  copied: number;
  /** A quick container change. */
  remux: number;
  /** Encoded again to H.264: the slow part. */
  reencode: number;
  alreadyThere: number;
  unsupported: number;
  reencodeVideoMs: number;
  /** Roughly how much disk the H.264 copies take, in `copiesDir`. */
  copyBytes: number;
  copiesDir: string;
  estimateSecs: number;
  /** "NVIDIA GPU", "CPU"…; null when nothing needs encoding. */
  encoder: string | null;
  /** No encoder works: files that need one will be skipped. */
  noEncoder: boolean;
}

/** `import-progress`: where a running import is. */
export interface ImportProgress {
  done: number;
  total: number;
  current: string | null;
  action: "importing" | "copying" | "remuxing" | "converting" | null;
  /** 0..1 through the current file's copy. */
  fileFraction: number | null;
  /** Times real time the current conversion runs. */
  speed: number | null;
  encoder: string | null;
  imported: number;
  converted: number;
  skipped: number;
  alreadyThere: number;
  elapsedSecs: number;
  etaSecs: number;
  /** 0..1 of the whole import, by time. */
  fraction: number;
}

/** A friend who watched one of your uploads. */
export interface ClipViewer {
  username: string;
  displayName?: string;
  viewedAt: string;
}

/** One of your uploads on the server, for My clips. */
/** How much server storage your own uploads use. */
export interface MyStorage {
  clips: number;
  bytes: number;
}

export interface MyUpload extends FeedClip {
  visibility: Visibility;
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

/** A part of Klipt on Settings → Resources. */
export type ResourcePart = "app" | "window" | "recorder" | "videoJobs";

export interface PartUsage {
  part: ResourcePart;
  /** How many processes of this part are running. */
  processes: number;
  ramBytes: number;
  /** Of the whole CPU (all threads), 0–100. */
  cpuPercent: number;
  /** Null when GPU numbers can't be read (see `gpuNote`). */
  gpuPercent: number | null;
  /** Which part of the GPU `gpuPercent` is of (its busiest process's); null when unknown or idle. */
  gpuEngine: "graphics" | "encoder" | "decoder" | null;
  vramBytes: number | null;
}

export interface ResourceUsage {
  /** Every part, in a fixed order, running or not. */
  parts: PartUsage[];
  totalRamBytes: number;
  cpuThreads: number;
  gpuAvailable: boolean;
  gpuNote: string | null;
  gpuName: string | null;
  totalVramBytes: number | null;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  /** Saves and applies settings: restarts the recorder, rebinds the shortcut, toggles autostart. */
  setSettings: (settings: Settings) => invoke<SettingsSaved>("set_settings", { settings }),
  listDisplays: () => invoke<Display[]>("list_displays"),
  /** Microphones (inputs only). */
  listAudioDevices: () => invoke<AudioDevice[]>("list_audio_devices"),
  openClipsFolder: () => invoke<void>("open_clips_folder"),
  /** Bytes the clips folder takes on disk. */
  clipsFolderSize: () => invoke<number>("clips_folder_size"),
  getAccount: () => invoke<Account | null>("get_account"),
  checkServer: (serverUrl: string) => invoke<string>("check_server", { serverUrl }),
  /** klipt.adamko.tech, or KLIPT_SERVER when set (development). */
  getDefaultServer: () => invoke<string>("get_default_server"),
  /** The server's sign-up page in the browser (works logged out). */
  openSignup: (server: string) => invoke<void>("open_signup", { server }),
  login: (serverUrl: string, username: string, password: string) =>
    invoke<Account>("login", { serverUrl, username, password }),
  logout: () => invoke<void>("logout"),
  /** The account as the server has it now (and tells it which version runs here); null when logged out. */
  refreshAccount: () => invoke<Account | null>("refresh_account"),
  /** Sets the name friends see; empty clears it. */
  updateProfile: (displayName: string) => invoke<Account>("update_profile", { displayName }),
  setProfileVisibility: (visibility: "friends" | "public") => invoke<Account>("set_profile_visibility", { visibility }),
  /** Signs out your other devices. */
  changePassword: (currentPassword: string, newPassword: string) =>
    invoke<void>("change_password", { currentPassword, newPassword }),
  /** What a problem report says about this PC. */
  systemInfo: () => invoke<InfoLine[]>("system_info"),
  /** Sends a problem or an idea to the server's admin; `includeLogs` adds system info and the log. */
  myAchievements: () => invoke<MyAchievements>("my_achievements"),
  setLooks: (looks: Partial<Looks>) => invoke<Account>("set_looks", { looks }),
  /** A square JPEG (the crop) as the profile picture. */
  setAvatar: (jpeg: Uint8Array) => invoke<Account>("set_avatar", { jpeg: Array.from(jpeg) }),
  removeAvatar: () => invoke<Account>("remove_avatar"),
  /** A picture file as a data: URL, to crop. */
  readPicture: (path: string) => invoke<string>("read_picture", { path }),
  sendFeedback: (kind: FeedbackKind, message: string, includeLogs: boolean) =>
    invoke<void>("send_feedback", { kind, message, includeLogs }),
  openLogsFolder: () => invoke<void>("open_logs_folder"),
  /** Running apps with shortcuts of their own (overlays) that may take Klipt's. */
  shortcutApps: () => invoke<ShortcutApp[]>("shortcut_apps"),
  getHotkeyStatus: () => invoke<HotkeyStatus>("get_hotkey_status"),
  saveClipNow: () => invoke<void>("save_clip_now"),
  /** Starts a full recording, or stops the running one (results arrive as events). */
  toggleFullRecording: () => invoke<void>("toggle_full_recording"),
  /** When the running full recording started (Unix ms), or null. */
  getFullRecording: () => invoke<number | null>("get_full_recording"),
  /** Takes a screenshot; it arrives as `clip-saved`. */
  takeScreenshot: () => invoke<void>("take_screenshot"),
  listClips: (filter: ListFilter) => invoke<ClipView[]>("list_clips", { filter }),
  listGames: () => invoke<GameCount[]>("list_games"),
  renameClip: (id: string, title: string) => invoke<void>("rename_clip", { id, title }),
  setClipGame: (id: string, name: string | null) => invoke<void>("set_clip_game", { id, name }),
  setClipFavorite: (id: string, favorite: boolean) => invoke<void>("set_clip_favorite", { id, favorite }),
  deleteClip: (id: string, deleteFile: boolean) => invoke<void>("delete_clip", { id, deleteFile }),
  showInFolder: (id: string) => invoke<void>("show_in_folder", { id }),
  /** Plays the clip's picture with no sound (the card's hover preview). */
  clipPreviewUrl: (id: string) => invoke<string>("clip_preview_url", { id }),
  /** All your uploads, newest first; null when not logged in. */
  myUploads: () => invoke<MyUpload[] | null>("my_uploads"),
  /** How much server storage your uploads use. */
  myStorage: () => invoke<MyStorage>("my_storage"),
  /** Renames your upload on the server and/or changes who sees it. */
  updateUpload: (id: string, change: { title?: string; visibility?: Visibility }) =>
    invoke<MyUpload>("update_upload", { id, title: change.title ?? null, visibility: change.visibility ?? null }),
  deleteUpload: (id: string) => invoke<void>("delete_upload", { id }),
  /** Sets your upload's thumbnail from a picture, cut to the clip's shape. */
  setUploadThumbnail: (upload: MyUpload, path: string) =>
    invoke<MyUpload>("set_upload_thumbnail", { id: upload.id, path, width: upload.width, height: upload.height }),
  /**
   * Makes a picture this clip's thumbnail (sent with each upload, and now to an uploaded clip);
   * null goes back to the frame. Returns why the online clip couldn't be updated, if it couldn't.
   */
  setThumbnailImage: (id: string, path: string | null) => invoke<string | null>("set_thumbnail_image", { id, path }),
  /** Downloads your upload into the library (linked to the server copy). */
  downloadUpload: (id: string) => invoke<ClipView>("download_upload", { id }),
  copyShareLink: (id: string) => invoke<string>("copy_share_link", { id }),
  /** `replace`: swap the video at the clip's existing link instead of making a new one. */
  pendingUploads: () => invoke<PendingUpload[]>("pending_uploads"),
  resumePendingUploads: () => invoke<void>("resume_pending_uploads"),
  uploadClip: (
    id: string,
    title: string,
    visibility: Visibility,
    tagged: string[] = [],
    replace = false,
    more = false,
    highQuality = false,
    albums: string[] = [],
  ) => invoke<void>("upload_clip", { id, title, visibility, tagged, replace, more, highQuality, albums }),
  /** The server's users, for tagging friends in a clip. */
  listUsers: () => invoke<Person[]>("list_users"),
  listFriends: () => invoke<FriendList>("list_friends"),
  addFriend: (username: string) => invoke<{ state: "outgoing" | "friends" | "incoming" | "none" }>("add_friend", { username }),
  removeFriend: (username: string) => invoke<void>("remove_friend", { username }),
  makeReferral: () => invoke<Referral>("make_referral"),
  listReferrals: () => invoke<Referral[]>("list_referrals"),
  revokeReferral: (code: string) => invoke<void>("revoke_referral", { code }),
  /** One page of the feed; `search` matches title, game and uploader on the server. */
  feedClips: (
    game: string | null,
    cursor: string | null,
    search: string | null = null,
    withMe = false,
    kind: "clip" | "screenshot" | null = null,
    favorites = false,
    /** Only this person's clips (their profile), as far as you may see them. */
    username: string | null = null,
  ) => invoke<FeedPage>("feed_clips", { game, cursor, search, withMe, kind, favorites, username }),
  /** What someone's profile head and hover card show. */
  userProfile: (username: string) => invoke<UserProfile>("user_profile", { username }),
  /** Looks at every file under `folder` (`import-check-progress`) and says what importing it
   * would do; null when stopped or an import is already running. */
  checkImport: (folder: string) => invoke<ImportPreview | null>("check_import", { folder }),
  /** Imports the folder `checkImport` just looked at, in the background (`import-*` events);
   * false when an import is already running. */
  importFolder: (folder: string) => invoke<boolean>("import_folder", { folder }),
  /** Like `checkImport`, for files and folders dropped on the window; with `copy` they're copied
   * into the clips folder instead of used where they are. */
  checkDrop: (paths: string[], copy: boolean) => invoke<ImportPreview | null>("check_drop", { paths, copy }),
  /** Imports what `checkDrop` just looked at, like `importFolder`. */
  importDrop: () => invoke<boolean>("import_drop"),
  /** Stops checking, or stops the import (the file being converted is dropped). */
  cancelImport: () => invoke<void>("cancel_import"),
  /** Stars or unstars a server clip (yours or a friend's); kept on the server. */
  /** A local URL that plays a server clip through the app's read-ahead cache (starts much faster). */
  playerVideoUrl: (url: string) => invoke<string>("player_video_url", { url }),
  setServerFavorite: (id: string, favorite: boolean) => invoke<void>("set_server_favorite", { id, favorite }),
  /** The feed's games, or (with `username`) that person's. */
  feedGames: (username: string | null = null) => invoke<FeedGame[]>("feed_games", { username }),
  /** Your albums, most recently changed first; with `clip` (a server clip id), which hold it. */
  listAlbums: (clip?: string) => invoke<AlbumSummary[]>("list_albums", { clip: clip ?? null }),
  /** A new album holding server clips `clipIds`, in that order. */
  createAlbum: (title: string, clipIds: string[]) => invoke<AlbumSummary>("create_album", { title, clipIds }),
  /** Puts a server clip at the end of an album, or (`add` false) takes it out. */
  setAlbumClip: (albumId: string, clipId: string, add: boolean) => invoke<void>("set_album_clip", { albumId, clipId, add }),
  /** One album with its clips, in order. */
  getAlbum: (id: string) => invoke<Album>("get_album", { id }),
  /** Renames your album and/or reorders it (`clipIds`: exactly the clips it holds). */
  updateAlbum: (id: string, change: { title?: string; clipIds?: string[] }) =>
    invoke<Album>("update_album", { id, title: change.title ?? null, clipIds: change.clipIds ?? null }),
  /** Deletes your album; its clips stay. */
  deleteAlbum: (id: string) => invoke<void>("delete_album", { id }),
  /** Friends' clips new since you last opened the Feed. */
  feedNewCount: () => invoke<number>("feed_new_count"),
  /** You've seen the feed up to the clip created at `newest`. Gives back what you'd seen up to
   *  before (null before the app's first feed check). */
  markFeedSeen: (newest: string) => invoke<string | null>("mark_feed_seen", { newest }),
  /** Counts a play of a server clip (best effort). */
  recordView: (id: string) => invoke<void>("record_view", { id }),
  /** Friends who watched your upload, first viewer first. */
  clipViewers: (id: string) => invoke<ClipViewer[]>("clip_viewers", { id }),
  copyText: (text: string) => invoke<void>("copy_text_to_clipboard", { text }),
  /** Opens the Klipt website in the browser; `path` like `/me`. */
  /** Downloads and installs the waiting update, then restarts Klipt. */
  installUpdate: () => invoke<void>("install_update"),
  /** Checks for a newer Klipt now; a found one also shows the update banner. */
  checkForUpdate: () => invoke<UpdateCheck>("check_for_update"),
  /** Which Klipt this is (Settings → About). */
  appInfo: () => invoke<{ version: string; commit: string | null; dev: boolean; platform: string }>("app_info"),
  /** The website's home page, or `url` (a link on the server, e.g. a clip's share link). */
  openWebsite: (url?: string) => invoke<void>("open_website", { url: url ?? null }),
  openEditor: (id: string) => invoke<EditorData>("open_editor", { id }),
  closeEditor: (id: string) => invoke<void>("close_editor", { id }),
  /** Saves the trim and volumes, and the thumbnail frame (null = automatic). */
  saveEdit: (id: string, edit: EditInput, thumbnailMs: number | null) =>
    invoke<void>("save_edit", { id, ...edit, thumbnailMs }),
  /** Resolves once the copy is written; progress arrives as `export-progress`. */
  exportCopy: (id: string, destPath: string, edit: EditInput) =>
    invoke<void>("export_copy", { id, destPath, ...edit }),
  /**
   * "New clip from this part": the trim of `edit` (with what's inside it) becomes a new clip
   * titled `title`; the source stays as it is. The new clip also arrives as `clip-imported`.
   */
  makePartClip: (id: string, title: string, edit: EditInput, thumbnailMs: number | null) =>
    invoke<ClipView>("make_part_clip", { id, title, ...edit, thumbnailMs }),
  /**
   * "Combine": clips `ids`, in this order and each with its saved edit, become one new clip
   * titled `title`; they stay as they are. Progress arrives as `combine-progress`; the new clip
   * also arrives as `clip-imported`.
   */
  combineClips: (ids: string[], title: string) => invoke<ClipView>("combine_clips", { ids, title }),
  /** What Klipt's processes use right now; made for polling about once a second. */
  resourceUsage: () => invoke<ResourceUsage>("resource_usage"),
  /** Closes the hidden window to free its memory; answers `release-window-check`. */
  releaseWindow: () => invoke<void>("release_window"),
  /** The update announced last (`update-available`), for a window that opened after it. */
  pendingUpdate: () => invoke<{ version: string; command: string | null } | null>("pending_update"),
  /** How many library clips (and bytes) sit in `from`, the old clips folder, to move to the current one. */
  clipsToMove: (from: string) => invoke<{ count: number; bytes: number }>("clips_to_move", { from }),
  /** Starts moving them; returns the move's job id (progress in the Jobs list, `clips-moved` at the end). */
  moveClips: (from: string) => invoke<number>("move_clips", { from }),
  /** Stops the move after the file it's on. */
  cancelMoveClips: () => invoke<void>("cancel_move_clips"),
  /** Background work since Klipt started (the Jobs screen); changes arrive as `jobs-changed`. */
  listJobs: () => invoke<Job[]>("list_jobs"),
  /** Drops the finished jobs; returns what's left. */
  clearFinishedJobs: () => invoke<Job[]>("clear_finished_jobs"),
};

export interface Events {
  "clip-saved": ClipView;
  /** A clip imported from a folder (no "saved" toast for these). */
  "clip-imported": ClipView;
  "import-check-progress": { checked: number; total: number };
  "import-progress": ImportProgress;
  "import-finished": ImportSummary;
  "save-busy": null;
  /** A full recording runs since `startedAtMs`. */
  "full-recording-started": { startedAtMs: number };
  /** The full recording ended; its `clip-saved` (if any) follows. */
  "full-recording-stopped": null;
  /** Every few seconds while a full recording runs (null: not known yet). */
  "full-recording-status": { sizeBytes: number | null; freeBytes: number | null };
  "recorder-error": AppError;
  "hotkey-bound": {
    display: string;
    screenshotDisplay: string | null;
    screenshotError: string | null;
    recordDisplay: string | null;
    recordError: string | null;
  };
  "hotkey-error": AppError;
  "upload-progress": UploadProgress;
  "upload-done": UploadDone;
  "upload-failed": UploadFailed;
  "upload-summary": UploadToast;
  "export-progress": ExportProgress;
  /** How far a "Combine" is, 0..1. */
  "combine-progress": number;
  /** Moving the clips folder ended: clips have new paths. */
  "clips-moved": null;
  /** The whole Jobs list, after any change. */
  "jobs-changed": Job[];
  /** The number of friends' clips you haven't seen changed. */
  "feed-new": { count: number };
  /** A clip's thumbnail was made again (the editor picked a new frame). */
  "thumbnail-changed": { id: string };
  /** More filmstrip frames of the open clip are readable. */
  "filmstrip-progress": { id: string; frames: number };
  /** How far `openEditor` got preparing a clip. */
  "editor-progress": { id: string } & EditorProgress;
  /** The automatic cleanup deleted old clips. */
  "clips-cleaned": { deleted: number; freedBytes: number };
  /**
   * A newer release is out. Windows installs it itself (`installUpdate`); on Linux `command` is
   * what the user runs in a terminal to install it.
   */
  "update-available": { version: string; command: string | null };
  /**
   * The window has been hidden a while. Answer with `releaseWindow` when closing it loses
   * nothing; it's asked again every minute while hidden.
   */
  "release-window-check": null;
  /** The window went to the tray or was minimized: stop anything playing. */
  "window-hidden": null;
  /** An achievement to show as a toast (the window is in front). */
  achievement: { title: string; body: string };
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

/**
 * Runs `handler` when the window goes to the tray or is minimized (the core's `window-hidden`,
 * plus the page becoming hidden where the web view reports it). Returns a stop function.
 */
export function onWindowHidden(handler: () => void): () => void {
  const stopCore = on("window-hidden", handler);
  const onVisibility = () => {
    if (document.hidden) handler();
  };
  document.addEventListener("visibilitychange", onVisibility);
  return () => {
    stopCore();
    document.removeEventListener("visibilitychange", onVisibility);
  };
}
