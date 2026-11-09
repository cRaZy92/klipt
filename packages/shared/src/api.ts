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
  createdAt: string;
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
