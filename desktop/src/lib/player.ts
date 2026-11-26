// The clip player's helpers: speeds, the time label, its keys and the remembered volume.

/** Playback speeds, in the order the speed button goes through them. */
export const SPEEDS = [0.5, 1, 1.5, 2] as const;

/** The speed after `current`; normal speed from one that isn't in the list. */
export function nextSpeed(current: number): number {
  const at = SPEEDS.indexOf(current as (typeof SPEEDS)[number]);
  return at < 0 ? 1 : SPEEDS[(at + 1) % SPEEDS.length];
}

/** "0:11", "1:05", "1:02:03". */
export function clockLabel(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const total = Math.floor(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

export type PlayerAction = "toggle" | "mute" | "back" | "forward" | "fullscreen" | "escape";

const KEYS: Record<string, PlayerAction> = {
  " ": "toggle",
  k: "toggle",
  m: "mute",
  j: "back",
  l: "forward",
  f: "fullscreen",
  escape: "escape",
};

/** The player action for a key press. The arrows are left to `navKey` (previous / next clip). */
export function playerKey(e: { key: string; ctrlKey?: boolean; altKey?: boolean; metaKey?: boolean }): PlayerAction | null {
  if (e.ctrlKey || e.altKey || e.metaKey) return null;
  return KEYS[e.key.toLowerCase()] ?? null;
}

export interface PlayerPrefs {
  volume: number;
  muted: boolean;
}

const PREFS_KEY = "klipt.player";
const DEFAULT_PREFS: PlayerPrefs = { volume: 1, muted: false };

/** The volume and mute the player had last time (just a convenience: missing storage is fine). */
export function loadPlayerPrefs(): PlayerPrefs {
  try {
    const saved = JSON.parse(localStorage.getItem(PREFS_KEY) ?? "null");
    if (!saved || typeof saved !== "object") return { ...DEFAULT_PREFS };
    const volume = typeof saved.volume === "number" && Number.isFinite(saved.volume) ? saved.volume : 1;
    return { volume: Math.min(1, Math.max(0, volume)), muted: saved.muted === true };
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

export function savePlayerPrefs(prefs: PlayerPrefs): void {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
  } catch {
    // Storage blocked: the next launch starts at full volume.
  }
}
