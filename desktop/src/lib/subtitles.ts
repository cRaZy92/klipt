// Subtitles in the editor: timed one-line texts on the Subtitles lane. The export
// (src-tauri/src/subtitles.rs) draws them onto the picture at the same moments.

import type { SubtitleRange } from "./api";
import type { TrimRange } from "./trim";
import { moveRange, resizeRange, roomAt } from "./ranges";

/** The shortest a subtitle can be. */
export const MIN_SUBTITLE_MS = 300;
/** How long a new subtitle is (shorter when something follows sooner). */
export const DEFAULT_SUBTITLE_MS = 2000;
/** The longest a subtitle's text can be, in characters. */
export const MAX_SUBTITLE_CHARS = 200;

/** The index of the subtitle showing at `ms`, or −1. */
export function subtitleIndexAt(subs: SubtitleRange[], ms: number): number {
  return subs.findIndex((s) => ms >= s.startMs && ms < s.endMs);
}

/** A new subtitle with no text yet at `ms`, or null when there's no room for one there. */
export function addSubtitleAt(subs: SubtitleRange[], ms: number, trim: TrimRange): SubtitleRange[] | null {
  const room = roomAt(subs, ms, trim, DEFAULT_SUBTITLE_MS, MIN_SUBTITLE_MS);
  if (!room) return null;
  return [...subs, { ...room, text: "" }].sort((a, b) => a.startMs - b.startMs);
}

export function moveSubtitle(subs: SubtitleRange[], i: number, startMs: number, trim: TrimRange): SubtitleRange[] {
  return moveRange(subs, i, startMs, trim);
}

export function resizeSubtitle(
  subs: SubtitleRange[],
  i: number,
  edge: "start" | "end",
  ms: number,
  trim: TrimRange,
): SubtitleRange[] {
  return resizeRange(subs, i, edge, ms, trim, MIN_SUBTITLE_MS);
}

/** `text` as a subtitle stores it: one trimmed line of at most MAX_SUBTITLE_CHARS characters. */
export function cleanText(text: string): string {
  const line = text.replace(/[\r\n]+/g, " ").trim();
  return Array.from(line).slice(0, MAX_SUBTITLE_CHARS).join("").trim();
}

/** Subtitle `i` with new text; blank text removes it. */
export function setSubtitleText(subs: SubtitleRange[], i: number, text: string): SubtitleRange[] {
  if (!subs[i]) return subs;
  const clean = cleanText(text);
  if (!clean) return removeSubtitle(subs, i);
  return subs.map((s, j) => (j === i ? { ...s, text: clean } : s));
}

export function removeSubtitle(subs: SubtitleRange[], i: number): SubtitleRange[] {
  return subs.filter((_, j) => j !== i);
}

/** The subtitles to save: one still waiting for its text is left out. */
export function savedSubtitles(subs: SubtitleRange[]): SubtitleRange[] {
  return subs.filter((s) => s.text !== "").map((s) => ({ startMs: s.startMs, endMs: s.endMs, text: s.text }));
}

/** Whether two subtitle lists are the same. */
export function sameSubtitles(a: SubtitleRange[], b: SubtitleRange[]): boolean {
  return (
    a.length === b.length &&
    a.every((s, i) => s.startMs === b[i].startMs && s.endMs === b[i].endMs && s.text === b[i].text)
  );
}
