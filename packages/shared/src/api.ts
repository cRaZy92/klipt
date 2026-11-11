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
  /** A video clip, or a screenshot whose `videoUrl` is the PNG. */
  kind: 'clip' | 'screenshot';
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
