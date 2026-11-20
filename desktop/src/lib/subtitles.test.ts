import { describe, expect, it } from "vitest";
import type { SubtitleRange } from "./api";
import {
  addSubtitleAt,
  cleanText,
  moveSubtitle,
  removeSubtitle,
  resizeSubtitle,
  sameSubtitles,
  savedSubtitles,
  setSubtitleText,
  subtitleIndexAt,
} from "./subtitles";

const trim = { start: 0, end: 20_000 };
const s = (startMs: number, endMs: number, text = "a"): SubtitleRange => ({ startMs, endMs, text });

describe("subtitleIndexAt", () => {
  it("finds the subtitle showing at a time", () => {
    const subs = [s(1000, 2000), s(2000, 3000)];
    expect(subtitleIndexAt(subs, 999)).toBe(-1);
    expect(subtitleIndexAt(subs, 1000)).toBe(0);
    expect(subtitleIndexAt(subs, 2000)).toBe(1);
    expect(subtitleIndexAt(subs, 3000)).toBe(-1);
  });
});

describe("addSubtitleAt", () => {
  it("adds an empty 2 s subtitle at the playhead", () => {
    expect(addSubtitleAt([], 5000, trim)).toEqual([s(5000, 7000, "")]);
  });
  it("is cut short before the next subtitle or the trim end", () => {
    expect(addSubtitleAt([s(6000, 8000)], 5000, trim)).toEqual([s(5000, 6000, ""), s(6000, 8000)]);
    expect(addSubtitleAt([], 19_000, trim)).toEqual([s(19_000, 20_000, "")]);
  });
  it("refuses with under 300 ms of room, inside a subtitle or outside the trim", () => {
    expect(addSubtitleAt([s(6000, 8000)], 5800, trim)).toBeNull();
    expect(addSubtitleAt([s(6000, 8000)], 7000, trim)).toBeNull();
    expect(addSubtitleAt([], 20_000, trim)).toBeNull();
  });
});

describe("moving and resizing", () => {
  it("can't pass a neighbour or the trim", () => {
    const subs = [s(1000, 2000), s(3000, 4000)];
    expect(moveSubtitle(subs, 1, 1500, trim)[1]).toEqual(s(2000, 3000));
    expect(moveSubtitle(subs, 1, 19_500, trim)[1]).toEqual(s(19_000, 20_000));
  });
  it("keeps at least 300 ms", () => {
    expect(resizeSubtitle([s(1000, 2000)], 0, "end", 1100, trim)[0]).toEqual(s(1000, 1300));
    expect(resizeSubtitle([s(1000, 2000)], 0, "start", 1900, trim)[0]).toEqual(s(1700, 2000));
  });
});

describe("text", () => {
  it("is one trimmed line of at most 200 characters", () => {
    expect(cleanText("  a\nb  ")).toBe("a b");
    expect(Array.from(cleanText("💀".repeat(250)))).toHaveLength(200);
  });
  it("setting blank text removes the subtitle", () => {
    expect(setSubtitleText([s(0, 1000)], 0, " hi\n")).toEqual([s(0, 1000, "hi")]);
    expect(setSubtitleText([s(0, 1000)], 0, "   ")).toEqual([]);
  });
  it("saves only subtitles with text", () => {
    expect(savedSubtitles([s(0, 1000, ""), s(1000, 2000, "b")])).toEqual([s(1000, 2000, "b")]);
  });
  it("compares lists", () => {
    expect(sameSubtitles([s(0, 1000)], [s(0, 1000)])).toBe(true);
    expect(sameSubtitles([s(0, 1000)], [s(0, 1000, "b")])).toBe(false);
    expect(sameSubtitles([], [s(0, 1000)])).toBe(false);
    expect(removeSubtitle([s(0, 1000), s(1000, 2000)], 0)).toEqual([s(1000, 2000)]);
  });
});
