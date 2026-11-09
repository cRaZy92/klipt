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
