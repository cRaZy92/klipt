import type { ImportSummary } from "./api";

function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** One line about a finished import, e.g. "Imported 38 clips (5 converted to H.264). 2 were already in Klipt." */
export function summaryText(s: ImportSummary): string {
  const parts: string[] = [];
  if (s.imported === 0) {
    parts.push(s.cancelled ? "Import stopped." : "Nothing new to import.");
  } else {
    const converted = s.converted > 0 ? ` (${s.converted} converted to H.264)` : "";
    parts.push(`${s.cancelled ? "Stopped after importing" : "Imported"} ${count(s.imported, "clip", "clips")}${converted}.`);
  }
  if (s.alreadyThere > 0) parts.push(`${count(s.alreadyThere, "was", "were")} already in Klipt.`);
  if (s.skipped.length > 0) parts.push(`${count(s.skipped.length, "file", "files")} skipped.`);
  return parts.join(" ");
}

/** A time left: "less than a minute", "about 4 min", "about 2 h 10 min". */
export function etaText(secs: number): string {
  if (!Number.isFinite(secs) || secs < 45) return "less than a minute";
  const min = Math.round(secs / 60);
  if (min < 60) return `about ${min} min`;
  const h = Math.floor(min / 60);
  const rest = min % 60;
  return rest === 0 ? `about ${h} h` : `about ${h} h ${rest} min`;
}

/** Time spent: "0:42", "12:05", "1:02:09". */
export function elapsedText(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  const two = (n: number) => String(n).padStart(2, "0");
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h}:${two(m)}:${two(s % 60)}` : `${m}:${two(s % 60)}`;
}

/** A length of video: "48 s", "12 min", "2 h 5 min". */
export function videoLengthText(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} s`;
  const min = Math.round(s / 60);
  if (min < 60) return `${min} min`;
  return `${Math.floor(min / 60)} h ${min % 60} min`;
}

/** "340 MB", "12.4 GB". */
export function sizeText(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb >= 10 ? Math.round(gb) : gb.toFixed(1)} GB`;
  return `${Math.max(1, Math.round(bytes / 1024 ** 2))} MB`;
}
