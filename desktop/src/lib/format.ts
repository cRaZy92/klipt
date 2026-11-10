// Display formatting for the library and upload dialog. Months are spelled out by hand so the
// output never depends on the system locale (some ICU versions write "Sept").

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function toDate(value: Date | number): Date {
  return value instanceof Date ? value : new Date(value);
}

const pad = (n: number) => String(n).padStart(2, "0");

/** 65000 → "1:05"; one hour or more → "1:01:05". */
export function formatDuration(ms: number): string {
  const total = Number.isFinite(ms) && ms > 0 ? Math.round(ms / 1000) : 0;
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${minutes}:${pad(seconds)}`;
}

/** "9 Nov 2026" */
export function formatDate(value: Date | number): string {
  const d = toDate(value);
  return `${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** "just now", "5 min ago", "3 h ago", "yesterday", "9 Nov" or "9 Nov 2025". */
export function relativeDate(value: Date | number, now: Date | number): string {
  const date = toDate(value);
  const current = toDate(now);
  const diff = current.getTime() - date.getTime();
  if (diff < MINUTE) return "just now";
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)} min ago`;
  if (diff < DAY) return `${Math.floor(diff / HOUR)} h ago`;
  const yesterday = new Date(current.getFullYear(), current.getMonth(), current.getDate() - 1);
  if (startOfDay(date) === yesterday.getTime()) return "yesterday";
  const dayMonth = `${date.getDate()} ${MONTHS[date.getMonth()]}`;
  return date.getFullYear() === current.getFullYear() ? dayMonth : `${dayMonth} ${date.getFullYear()}`;
}

/** The upload title suggestion: "Active Matter — 9 Nov 2026", or "Clip — 9 Nov 2026". */
export function defaultTitle(game: string | null | undefined, date: Date | number): string {
  const name = game?.trim() || "Clip";
  return `${name} — ${formatDate(date)}`;
}

function fileStem(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/**
 * The title the upload dialog suggests. A fresh clip is titled after its game or file name by
 * the save flow; those get `defaultTitle`. A title the user typed is kept.
 */
export function uploadTitle(clip: {
  title: string;
  gameName: string | null;
  path: string;
  createdAt: number;
}): string {
  const automatic = clip.title === (clip.gameName ?? "") || clip.title === fileStem(clip.path) || !clip.title.trim();
  return automatic ? defaultTitle(clip.gameName, clip.createdAt) : clip.title;
}

const KEY_NAMES: Record<string, string> = {
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

/** "Alt+Backquote" → "Alt + `" (Settings store keys by position, W3C `code` names). */
export function hotkeyLabel(accel: string): string {
  return accel
    .split("+")
    .map((part) => KEY_NAMES[part] ?? part.replace(/^(Key|Digit)(.)$/, "$2"))
    .join(" + ");
}

/** 30 → "30 seconds", 120 → "2 minutes". */
export function bufferLabel(seconds: number): string {
  if (seconds >= 60 && seconds % 60 === 0) {
    const minutes = seconds / 60;
    return minutes === 1 ? "1 minute" : `${minutes} minutes`;
  }
  return `${seconds} seconds`;
}

/** The suggested file name for "Export copy…": the title without characters files can't have. */
export function exportFileName(title: string): string {
  const name = title
    .replace(/[\\/:*?"<>|\u0000-\u001f]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[. ]+$/, "");
  return `${name || "Clip"}.mp4`;
}
