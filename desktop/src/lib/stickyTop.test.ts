import { describe, expect, it } from "vitest";
import { compactAt, compactShifts } from "./stickyTop";

describe("compactAt", () => {
  it("goes compact only once scrolled well down", () => {
    expect(compactAt(20, false)).toBe(false);
    expect(compactAt(60, false)).toBe(true);
  });

  it("stays compact until back near the top", () => {
    expect(compactAt(20, true)).toBe(true);
    expect(compactAt(4, true)).toBe(false);
  });
});

describe("compactShifts", () => {
  it("saves the padding, the title's shrink, the subtitle and the gap", () => {
    // 16px rem: padding 13.6, title 40 → 12 saved, subtitle 20, gap 8.
    const s = compactShifts(16, 40, 20);
    expect(s.title).toBeCloseTo(13.6);
    expect(s.tools).toBeCloseTo(13.6 + 12 + 20);
    expect(s.rest).toBeCloseTo(13.6 + 12 + 20 + 8);
  });
});
