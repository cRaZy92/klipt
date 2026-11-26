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
    // 16px rem: padding 13.6, title 40 → 12 saved, subtitle 20, gap 8; tools 20, shorter than the title.
    const s = compactShifts(16, 40, 20, 20);
    expect(s.title).toBeCloseTo(13.6);
    // Tools centred on the 28px compact title: top at 14.4 + 4, from 28 + 60 - 20 at rest.
    expect(s.tools).toBeCloseTo(68 - 18.4);
    expect(s.rest).toBeCloseTo(13.6 + 12 + 20 + 8);
  });

  it("keeps tools taller than the shrunken title off the top edge", () => {
    // 16px rem: title 40 (28 compact), subtitle 20, tools 38. Compact row is 38 tall from 14.4.
    const s = compactShifts(16, 40, 20, 38);
    const toolsTopRest = 28 + 60 - 38;
    expect(toolsTopRest - s.tools).toBeCloseTo(14.4);
    // The title is centred on the tools: (38 - 28) / 2 below the compact padding.
    expect(28 - s.title).toBeCloseTo(14.4 + 5);
    // Everything under the heading starts right below the compact row.
    expect(28 + 60 - s.rest).toBeCloseTo(14.4 + 38 - 8);
  });
});
