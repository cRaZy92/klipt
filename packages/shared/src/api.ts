import type { ActivityCounter, ActivityFirst } from './achievements.js';
import type { CosmeticId, Looks } from './looks.js';

export interface HealthResponse {
  ok: true;
}

export interface LoginRequest {
  username: string;
  password: string;
  client: 'web' | 'desktop';
  deviceName?: string;
}

export interface Me {
  id: string;
  username: string;
  /** The name friends see: the display name, or the username when none is set. */
  displayName: string;
  /** The display name as set, null when none is (`PATCH /api/me`). */
  customName: string | null;
  isAdmin: boolean;
  /** Your picture; null draws the initial (`PUT`/`DELETE /api/me/avatar`). */
  avatarUrl: string | null;
  /** What you picked (`PUT /api/me/looks`). */
  looks: Looks;
  /** Achievements you haven't seen yet. */
  newAchievements: number;
}

/** A person as lists show them: name, picture and the frame they wear. */
export interface UserRef {
  username: string;
  displayName: string;
  /** Null: draw the initial. */
  avatarUrl: string | null;
  /** Their equipped avatar frame, or null. */
  frame: string | null;
}

/** `PUT /api/me/looks`: only the fields sent change; a locked one is refused (403 `not_unlocked`). */
export type UpdateLooksRequest = Partial<Looks>;

/**
 * `POST /api/me/activity`: what only the desktop app knows. Counters are totals (the server keeps
 * the highest it has seen), firsts are kept forever. `backfill`: the first report from this
 * account, built from what the app already had, so its awards arrive quietly.
 */
export interface ActivityReport {
  counters: Partial<Record<ActivityCounter, number>>;
  firsts: ActivityFirst[];
  backfill?: boolean;
}

export interface EarnedAchievement {
  id: string;
  name: string;
  description: string;
  icon: string;
  earnedAt: string;
  seen: boolean;
  /** Earned for past activity (startup run or the app's first report): announced in one summary. */
  quiet: boolean;
  reward: CosmeticId | null;
  rewardName: string | null;
}

/** `GET /api/me/achievements` (also the answer to an activity report). */
export interface MyAchievements {
  /** Newest first. */
  earned: EarnedAchievement[];
  looks: Looks;
  unlocked: CosmeticId[];
}

/** `PATCH /api/me`: a blank or null display name clears it. */
export interface UpdateMeRequest {
  displayName?: string | null;
}

/** `POST /api/me/password`: other sessions are signed out. */
export interface ChangePasswordRequest {
  currentPassword: string;
  newPassword: string;
}

/** `POST /api/me/device`: what the desktop app reports about its PC, shown to admins. */
export interface DeviceReport {
  appVersion: string;
  os: string;
  gpu?: string | null;
}

/** `POST /api/feedback`: a problem report or a feature idea. */
export interface FeedbackRequest {
  kind: 'bug' | 'idea';
  message: string;
  source: 'desktop' | 'web';
  /** Version, OS, CPU, GPU… as label → value. */
  system?: Record<string, string>;
  /** The end of the app's log. */
  logs?: string;
}

/** A report on the admin page. */
export interface FeedbackItem {
  id: string;
  kind: 'bug' | 'idea';
  message: string;
  source: 'desktop' | 'web';
  system: Record<string, string> | null;
  hasLogs: boolean;
  createdAt: string;
  resolvedAt: string | null;
  user: { username: string; displayName: string };
}

/** `POST /api/admin/users/:id/password-reset`: a one-time link for the user to set a new password. */
export interface PasswordReset {
  url: string;
  expiresAt: string;
}

/** Web login returns `Me`; desktop login returns `DesktopLoginResponse`. */
export interface DesktopLoginResponse {
  token: string;
  me: Me;
}
export type LoginResponse = Me | DesktopLoginResponse;

export interface SessionInfo {
  id: string;
  kind: 'web' | 'desktop';
  label: string | null;
  createdAt: string;
  lastUsedAt: string;
  current: boolean;
}

export interface Invite {
  code: string;
  url: string;
  expiresAt: string;
}

export interface RegisterRequest {
  inviteCode: string;
  username: string;
  password: string;
}

export interface AdminUser {
  id: string;
  username: string;
  displayName: string;
  /** The last time any of their sessions was used. */
  lastSeenAt: string | null;
  /** Their most recently used desktop app: version, OS and GPU it reported. */
  app: { version: string | null; os: string | null; gpu: string | null; lastUsedAt: string } | null;
  avatarUrl: string | null;
  isAdmin: boolean;
  disabled: boolean;
  clipCount: number;
  /** Bytes the user's clips take on the server. */
  storageBytes: number;
  createdAt: string;
}

/** `GET /api/me/storage`: how much server storage your own clips use (feed and link-only). */
export interface MyStorage {
  clips: number;
  bytes: number;
}

/** A clip in the compression queue or recently compressed, for the admin page. */
export interface TranscodeJob {
  clipId: string;
  title: string;
  owner: string;
  state: 'queued' | 'running' | 'done' | 'skipped' | 'failed';
  durationMs: number;
  /** The stored file's size (the compressed one once `done`). */
  sizeBytes: number;
  /** The upload's size, once compression ran. */
  originalBytes: number | null;
  error: string | null;
  startedAt: string | null;
  finishedAt: string | null;
  /** 0..1 while running. */
  progress: number | null;
}

/** `GET /api/admin/transcodes`: the compression queue. */
export interface AdminTranscodes {
  enabled: boolean;
  counts: { queued: number; running: number; done: number; skipped: number; failed: number };
  savedBytes: number;
  /** The running job, then the queue in the order it'll be worked (newest upload first). */
  active: TranscodeJob[];
  /** Finished jobs, latest first. */
  recent: TranscodeJob[];
}

export interface AdminStats {
  users: number;
  clips: number;
  storageBytes: number;
  /** Free space on the server's data disk; null when it can't be read. */
  freeBytes: number | null;
  /** Under MIN_FREE_MB free: new uploads and compression wait. */
  diskLow: boolean;
}

export type Visibility = 'feed' | 'link';

export interface FinalizeRequest {
  uploadId: string;
  title: string;
  gameName?: string | null;
  steamAppId?: number | null;
  visibility?: Visibility;
  /** The frame (ms into the uploaded video) to use as the thumbnail; omitted = automatic. */
  thumbnailMs?: number | null;
  /** Friends in the clip, by username. */
  tagged?: string[];
  /** One of your clips whose video this upload replaces (same link). */
  replaces?: string;
  /** `high` (admins only): kept as uploaded, never compressed by the server. */
  quality?: 'standard' | 'high';
}

export interface Clip {
  id: string;
  title: string;
  gameName: string | null;
  steamAppId: number | null;
  visibility: Visibility;
  durationMs: number;
  width: number;
  height: number;
  sizeBytes: number;
  /** Friends who watched it (each once, never the owner) plus logged-out plays. */
  views: number;
  /** Friends in the clip, by username (alphabetical). */
  tagged: string[];
  /** Their display names, in the same order as `tagged`. */
  taggedNames: string[];
  /** You starred it (`PUT`/`DELETE /api/clips/:id/favorite`); list only those with `favorites=1`. */
  favorite: boolean;
  /** A video clip, or a screenshot whose `videoUrl` is the PNG. */
  kind: 'clip' | 'screenshot';
  /**
   * The server is still compressing the clip: `queued` waits its turn, `running` is being
   * compressed. It plays meanwhile (the uploaded file); null once it's done or for screenshots.
   */
  processing: 'queued' | 'running' | null;
  /**
   * The share link is ready to hand out: its Discord embed works. False for a few minutes after a
   * long clip is uploaded (its embed copy is being made), so the app waits before copying it.
   */
  shareReady: boolean;
  createdAt: string;
  owner: UserRef;
  shareUrl: string;
  videoUrl: string;
  thumbnailUrl: string;
}

export interface ClipList {
  items: Clip[];
  nextCursor: string | null;
}

export interface GameCount {
  gameName: string;
  steamAppId: number | null;
  count: number;
}

export interface UpdateClipRequest {
  title?: string;
  visibility?: Visibility;
  gameName?: string | null;
  steamAppId?: number | null;
}

/** `GET /api/clips/:id/viewers` (owner only): friends who watched the clip, first viewer first. */
export interface ClipViewer extends UserRef {
  viewedAt: string;
}

/** The most clips an album holds. */
export const MAX_ALBUM_CLIPS = 200;

/** An album as lists show it: `GET /api/albums`. */
export interface AlbumSummary {
  id: string;
  title: string;
  owner: UserRef;
  clipCount: number;
  /** The first clip's thumbnail; null while the album is empty. */
  coverUrl: string | null;
  /** The album's page, `/a/<id>`: anyone with the link can watch it, like a clip's share link. */
  shareUrl: string;
  createdAt: string;
  updatedAt: string;
  /** With `?clip=<id>`: whether that clip is in the album. */
  hasClip?: boolean;
}

/** An album with its clips, in order: `GET /api/albums/:id`. */
export interface Album extends AlbumSummary {
  clips: Clip[];
}
