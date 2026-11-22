import { describe, expect, it } from "vitest";
import type { AudioRange } from "./api";
import {
  addPart,
  gainAt,
  gainPoints,
  levelLabel,
  movePart,
  nextLevel,
  partsOn,
  removePart,
  resizePart,
  sameParts,
  setPartLevel,
  spanPart,
} from "./audioParts";

const trim = { start: 0, end: 20_000 };
const p = (track: number, startMs: number, endMs: number, level = 0): AudioRange => ({ track, startMs, endMs, level });

describe("addPart", () => {
  it("adds a muted 2 s part at the playhead, kept sorted by track then time", () => {
    const got = addPart([p(0, 5000, 6000), p(2, 0, 1000)], 1, 3000, trim);
    expect(got).toEqual({ parts: [p(0, 5000, 6000), p(1, 3000, 5000), p(2, 0, 1000)], index: 1 });
  });

  it("only looks at its own track", () => {
    expect(addPart([p(0, 1000, 4000)], 1, 2000, trim)?.parts).toEqual([p(0, 1000, 4000), p(1, 2000, 4000)]);
  });

  it("fits before the next part, and refuses where there is no room", () => {
    expect(addPart([p(0, 3500, 5000)], 0, 3000, trim)?.parts[0]).toEqual(p(0, 3000, 3500));
    expect(addPart([p(0, 3100, 5000)], 0, 3000, trim)).toBeNull();
    expect(addPart([p(0, 1000, 5000)], 0, 3000, trim)).toBeNull();
    expect(addPart([], 0, 20_000, trim)).toBeNull();
  });
});

describe("spanPart", () => {
  const parts = [p(0, 1000, 2000), p(0, 5000, 5200), p(0, 9000, 9500), p(1, 0, 20_000)];

  it("stretches either way from the anchor, up to the neighbours on its track", () => {
    expect(spanPart(parts, 1, 5000, 7000, trim)[1]).toEqual(p(0, 5000, 7000));
    expect(spanPart(parts, 1, 5000, 12_000, trim)[1]).toEqual(p(0, 5000, 9000));
    expect(spanPart(parts, 1, 5000, 3000, trim)[1]).toEqual(p(0, 3000, 5000));
    expect(spanPart(parts, 1, 5000, 0, trim)[1]).toEqual(p(0, 2000, 5000));
  });

  it("stays at least 200 ms long", () => {
    expect(spanPart(parts, 1, 5000, 5050, trim)[1]).toEqual(p(0, 5000, 5200));
    expect(spanPart(parts, 1, 5000, 4950, trim)[1]).toEqual(p(0, 4800, 5000));
  });

  it("returns the same list when nothing changes", () => {
    expect(spanPart(parts, 1, 5000, 5100, trim)).toBe(parts);
  });
});

describe("movePart and resizePart", () => {
  const parts = [p(0, 1000, 2000), p(0, 5000, 6000), p(1, 1500, 2500)];

  it("move within the track, never over a neighbour or out of the trim", () => {
    expect(movePart(parts, 1, 3000, trim)[1]).toEqual(p(0, 3000, 4000));
    expect(movePart(parts, 1, 0, trim)[1]).toEqual(p(0, 2000, 3000));
    expect(movePart(parts, 2, 0, trim)[2]).toEqual(p(1, 0, 1000));
    expect(movePart(parts, 2, 19_500, trim)[2]).toEqual(p(1, 19_000, 20_000));
  });

  it("resize keeps 200 ms and stops at a neighbour", () => {
    expect(resizePart(parts, 1, "start", 0, trim)[1]).toEqual(p(0, 2000, 6000));
    expect(resizePart(parts, 1, "end", 5100, trim)[1]).toEqual(p(0, 5000, 5200));
    expect(resizePart(parts, 0, "end", 9000, trim)[0]).toEqual(p(0, 1000, 5000));
  });
});

describe("levels", () => {
  it("go Muted → 25% → 50% → Muted", () => {
    expect([0, 0.25, 0.5, 0.33].map(nextLevel)).toEqual([0.25, 0.5, 0, 0]);
    expect([0, 0.25, 0.5].map(levelLabel)).toEqual(["Muted", "25%", "50%"]);
    expect(setPartLevel([p(0, 0, 1000)], 0, 0.5)).toEqual([p(0, 0, 1000, 0.5)]);
  });
});

describe("partsOn, removePart, sameParts", () => {
  it("work on the whole list", () => {
    const parts = [p(0, 0, 1000), p(1, 0, 1000), p(1, 2000, 3000)];
    expect(partsOn(parts, 1).map((x) => x.i)).toEqual([1, 2]);
    expect(removePart(parts, 1)).toEqual([p(0, 0, 1000), p(1, 2000, 3000)]);
    expect(sameParts(parts, parts.map((x) => ({ ...x })))).toBe(true);
    expect(sameParts(parts, setPartLevel(parts, 0, 0.25))).toBe(false);
  });
});

describe("gainAt and gainPoints", () => {
  const parts = [p(0, 1000, 2000, 0), p(0, 3000, 3040, 0.5)];

  it("fade 30 ms into and out of a part (half of a short one)", () => {
    expect(gainAt(parts, 999)).toBe(1);
    expect(gainAt(parts, 1015)).toBeCloseTo(0.5);
    expect(gainAt(parts, 1500)).toBe(0);
    expect(gainAt(parts, 1985)).toBeCloseTo(0.5);
    expect(gainAt(parts, 2000)).toBe(1);
    expect(gainAt(parts, 3020)).toBeCloseTo(0.5);
  });

  it("give the corners of the gain between two times", () => {
    expect(gainPoints(parts, 0, 2500)).toEqual([
      { ms: 0, gain: 1 },
      { ms: 1000, gain: 1 },
      { ms: 1030, gain: 0 },
      { ms: 1970, gain: 0 },
      { ms: 2000, gain: 1 },
      { ms: 2500, gain: 1 },
    ]);
    expect(gainPoints(parts, 1500, 1600)).toEqual([
      { ms: 1500, gain: 0 },
      { ms: 1600, gain: 0 },
    ]);
  });
});
