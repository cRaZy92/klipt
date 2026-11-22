import { describe, expect, it } from "vitest";
import { frameIndex, frameUrl, previewIndex, tiles } from "./filmstrip";

const asPath = (p: string) => p;

describe("frameIndex", () => {
  it("rounds to the nearest 100 ms picture", () => {
    expect(frameIndex(0)).toBe(0);
    expect(frameIndex(149)).toBe(1);
    expect(frameIndex(150)).toBe(2);
  });

  it("never goes below the first picture", () => {
    expect(frameIndex(-5)).toBe(0);
  });
});

describe("previewIndex", () => {
  it("is the picture at ms while it is ready", () => {
    expect(previewIndex(1000, 11)).toBe(10);
  });

  it("is null while that picture isn't made yet", () => {
    expect(previewIndex(1000, 10)).toBeNull();
    expect(previewIndex(0, 0)).toBeNull();
  });
});

describe("frameUrl", () => {
  it("names the picture with its index padded to 6 digits", () => {
    expect(frameUrl("/c/edit/a-strip", 7, asPath)).toMatch(/000007\.jpg$/);
    expect(frameUrl("/c/edit/a-strip", 7, asPath)).toBe("/c/edit/a-strip/000007.jpg");
  });

  it("joins with the separator a Windows dir uses", () => {
    expect(frameUrl("C:\\Klipt\\edit\\a-strip", 123, asPath)).toBe("C:\\Klipt\\edit\\a-strip\\000123.jpg");
  });

  it("joins with / when the dir mixes separators", () => {
    expect(frameUrl("C:\\Klipt/edit/a-strip", 1, asPath)).toBe("C:\\Klipt/edit/a-strip/000001.jpg");
  });

  it("turns the path into a URL", () => {
    expect(frameUrl("/strip", 0, (p) => `asset://${p}`)).toBe("asset:///strip/000000.jpg");
  });
});

describe("tiles", () => {
  it("covers the track with ceil(track / tile) tiles, each with its middle's time", () => {
    const t = tiles(1000, 100, 10_000);
    expect(t).toHaveLength(10);
    expect(t[0]).toEqual({ i: 0, leftPct: 0, widthPct: 10, ms: 500 });
    expect(t[9].ms).toBe(9500);
  });

  it("lets the last tile run past the end and takes its middle inside the clip", () => {
    const t = tiles(250, 100, 10_000);
    expect(t).toHaveLength(3);
    expect(t[2].leftPct).toBeCloseTo(80);
    expect(t[2].widthPct).toBeCloseTo(40);
    expect(t[2].ms).toBe(9000);
  });

  it("shows only the tiles inside a zoomed-in view, fixed in time so they slide when it pans", () => {
    // 1000 px showing 2–4 s: a 100 px tile is 200 ms long.
    const t = tiles(1000, 100, 10_000, { from: 2_050, to: 4_050 });
    expect(t[0]).toMatchObject({ i: 10, ms: 2_100 });
    expect(t[0].leftPct).toBeCloseTo(-2.5);
    expect(t[0].widthPct).toBeCloseTo(10);
    expect(t.at(-1)!.i).toBe(20);
    expect(t).toHaveLength(11);
  });

  it("never adds a tile past the clip's end through rounding", () => {
    const t = tiles(300, 100, 1000);
    expect(t).toHaveLength(3);
    expect(t.every((x) => x.ms < 1000)).toBe(true);
  });

  it("is empty for a track with no width or tiles with no width", () => {
    expect(tiles(0, 100, 10_000)).toEqual([]);
    expect(tiles(1000, 0, 10_000)).toEqual([]);
  });
});
