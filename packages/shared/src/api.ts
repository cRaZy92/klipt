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
