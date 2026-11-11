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
  isAdmin: boolean;
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
  /** You starred it (`PUT`/`DELETE /api/clips/:id/favorite`); list only those with `favorites=1`. */
  favorite: boolean;
  /** A video clip, or a screenshot whose `videoUrl` is the PNG. */
  kind: 'clip' | 'screenshot';
  /**
   * The server is still compressing the clip: `queued` waits its turn, `running` is being
   * compressed. It plays meanwhile (the uploaded file); null once it's done or for screenshots.
   */
  processing: 'queued' | 'running' | null;
  createdAt: string;
  owner: { username: string };
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
export interface ClipViewer {
  username: string;
  viewedAt: string;
}
