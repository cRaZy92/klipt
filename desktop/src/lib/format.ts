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

/** 0.4 MB, 9.5 MB, 340 MB, 1.5 GB, 12 GB: one decimal below 10, a trailing ".0" dropped. */
export function formatSize(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  const [value, unit] = gb >= 1 ? [gb, "GB"] : [bytes / 1024 ** 2, "MB"];
  const shown =
    value >= 10 ? String(Math.round(value)) : (bytes > 0 ? Math.max(value, 0.1) : 0).toFixed(1).replace(/\.0$/, "");
  return `${shown} ${unit}`;
}

/** "1 view", "12 views". */
export function viewsLabel(views: number): string {
  return views === 1 ? "1 view" : `${views} views`;
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

const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/**
 * The heading a clip is listed under: "Today", "Yesterday", "Last 7 days", "Earlier this month",
 * then the month ("October"), with the year when it isn't this year ("October 2025").
 */
export function dayGroup(value: Date | number, now: Date | number): string {
  const date = toDate(value);
  const current = toDate(now);
  // Rounded, so a day with a daylight-saving change still counts as one day.
  const days = Math.round((startOfDay(current) - startOfDay(date)) / DAY);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return "Last 7 days";
  const sameYear = date.getFullYear() === current.getFullYear();
  if (sameYear && date.getMonth() === current.getMonth()) return "Earlier this month";
  const month = MONTH_NAMES[date.getMonth()];
  return sameYear ? month : `${month} ${date.getFullYear()}`;
}

export interface DayGroup<T> {
  /** Unique within one list, for keyed blocks. */
  key: string;
  label: string;
  items: T[];
}

/**
 * Splits an already sorted list into runs that share a `dayGroup` heading, keeping the order.
 * Keys are the labels (stable when a group before them goes away); a label that comes back
 * in an unsorted list gets a suffix.
 */
export function groupByDay<T>(items: T[], dateOf: (item: T) => Date | number, now: Date | number): DayGroup<T>[] {
  const groups: DayGroup<T>[] = [];
  const seen = new Map<string, number>();
  for (const item of items) {
    const label = dayGroup(dateOf(item), now);
    const last = groups.at(-1);
    if (last?.label === label) {
      last.items.push(item);
      continue;
    }
    const times = seen.get(label) ?? 0;
    seen.set(label, times + 1);
    groups.push({ key: times === 0 ? label : `${label}#${times}`, label, items: [item] });
  }
  return groups;
}

// Hues far enough apart to tell at a glance; red is left out, it means danger.
const HUES = [262, 212, 188, 158, 122, 84, 46, 24, 336, 298];

/** A stable hue (0–359) for a name, so a game or a friend has the same colour everywhere. */
export function hueOf(name: string): number {
  let hash = 0;
  for (const ch of name.trim().toLowerCase()) hash = (hash * 31 + ch.codePointAt(0)!) >>> 0;
  return HUES[hash % HUES.length];
}

/** A shortcut as separate keys for keycaps: "Alt + `" → ["Alt", "`"]. A "+" key is kept. */
export function hotkeyKeys(label: string): string[] {
  const text = label.trim();
  const keys = text
    .split("+")
    .map((key) => key.trim())
    .filter(Boolean);
  if (/(^|\+)\s*\+$/.test(text)) keys.push("+");
  return keys;
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
