import { describe, expect, it } from "vitest";
import type { BlurRange } from "./api";
import { addBlurAt, blurIndexAt, moveBlur, removeBlur, resizeBlur, sameBlurs } from "./blur";

const b = (startMs: number, endMs: number): BlurRange => ({ startMs, endMs });
const TRIM = { start: 0, end: 20_000 };

describe("blurIndexAt", () => {
  it("finds the part playing at a moment (its end is outside)", () => {
    const blurs = [b(1000, 3000), b(5000, 6000)];
    expect(blurIndexAt(blurs, 999)).toBe(-1);
    expect(blurIndexAt(blurs, 1000)).toBe(0);
    expect(blurIndexAt(blurs, 3000)).toBe(-1);
    expect(blurIndexAt(blurs, 5500)).toBe(1);
  });
});

describe("addBlurAt", () => {
  it("adds a 3 s part at the playhead, sorted", () => {
    expect(addBlurAt([b(8000, 9000)], 1000, TRIM)).toEqual([b(1000, 4000), b(8000, 9000)]);
  });

  it("fits before the next part or the trim end", () => {
    expect(addBlurAt([b(2000, 3000)], 1000, TRIM)).toEqual([b(1000, 2000), b(2000, 3000)]);
    expect(addBlurAt([], 19_000, TRIM)).toEqual([b(19_000, 20_000)]);
  });

  it("refuses inside a part, with no room, or outside the trim", () => {
    expect(addBlurAt([b(1000, 3000)], 2000, TRIM)).toBeNull();
    expect(addBlurAt([b(1200, 3000)], 1000, TRIM)).toBeNull();
    expect(addBlurAt([], 500, { start: 1000, end: 5000 })).toBeNull();
    expect(addBlurAt([], 19_900, TRIM)).toBeNull();
  });
});

describe("moving and resizing", () => {
  const blurs = [b(1000, 2000), b(3000, 4000), b(6000, 7000)];

  it("moves between the neighbours", () => {
    expect(moveBlur(blurs, 1, 4500, TRIM)[1]).toEqual(b(4500, 5500));
    expect(moveBlur(blurs, 1, 1500, TRIM)[1]).toEqual(b(2000, 3000));
  });

  it("resizes down to the shortest part, not past the neighbours", () => {
    expect(resizeBlur(blurs, 1, "start", 3900, TRIM)[1]).toEqual(b(3700, 4000));
    expect(resizeBlur(blurs, 1, "end", 9000, TRIM)[1]).toEqual(b(3000, 6000));
  });
});

describe("removeBlur and sameBlurs", () => {
  it("removes one part and compares lists", () => {
    const blurs = [b(1000, 2000), b(3000, 4000)];
    expect(removeBlur(blurs, 0)).toEqual([b(3000, 4000)]);
    expect(sameBlurs(blurs, [b(1000, 2000), b(3000, 4000)])).toBe(true);
    expect(sameBlurs(blurs, [b(1000, 2000), b(3000, 4500)])).toBe(false);
    expect(sameBlurs(blurs, [b(1000, 2000)])).toBe(false);
  });
});
