import { describe, expect, it } from "vitest";
import { blockLayout, laneShading, rulerStep, rulerTicks, toggleMute } from "./lanes";

describe("rulerStep", () => {
  it("labels every 5 s under 30 s", () => {
    expect(rulerStep(12_000)).toBe(5000);
    expect(rulerStep(29_999)).toBe(5000);
  });

  it("labels every 10 s from 30 s up to 3 min", () => {
    expect(rulerStep(30_000)).toBe(10_000);
    expect(rulerStep(180_000)).toBe(10_000);
  });

  it("labels every 30 s over 3 min", () => {
    expect(rulerStep(180_001)).toBe(30_000);
    expect(rulerStep(600_000)).toBe(30_000);
  });
});

describe("rulerStep on a zoomed-in or long timeline", () => {
  it("labels at most 10 times under 10 s, down to every 0.1 s", () => {
    expect(rulerStep(9_000)).toBe(1000);
    expect(rulerStep(4_000)).toBe(500);
    expect(rulerStep(1_000)).toBe(100);
  });

  it("labels at most 20 times on long recordings", () => {
    expect(rulerStep(1_200_000)).toBe(60_000);
    expect(rulerStep(3_600_000)).toBe(300_000);
  });
});

describe("rulerTicks", () => {
  it("places a label at each step from 0, as m:ss at its share of the clip", () => {
    expect(rulerTicks(50_000)).toEqual([
      { ms: 0, pct: 0, label: "0:00" },
      { ms: 10_000, pct: 20, label: "0:10" },
      { ms: 20_000, pct: 40, label: "0:20" },
      { ms: 30_000, pct: 60, label: "0:30" },
      { ms: 40_000, pct: 80, label: "0:40" },
    ]);
  });

  it("leaves out a label at the clip's very end", () => {
    expect(rulerTicks(20_000).map((t) => t.label)).toEqual(["0:00", "0:05", "0:10", "0:15"]);
  });

  it("formats minutes", () => {
    expect(rulerTicks(240_000).map((t) => t.label)).toEqual([
      "0:00", "0:30", "1:00", "1:30", "2:00", "2:30", "3:00", "3:30",
    ]);
  });

  it("labels only the zoomed-in view, at its share of the screen", () => {
    const ticks = rulerTicks(60_000, { from: 10_500, to: 19_500 });
    expect(ticks).toHaveLength(9);
    expect(ticks.slice(0, 2)).toEqual([
      { ms: 11_000, pct: (500 / 9000) * 100, label: "0:11" },
      { ms: 12_000, pct: (1500 / 9000) * 100, label: "0:12" },
    ]);
  });

  it("labels tenths when the steps are under a second", () => {
    expect(rulerTicks(60_000, { from: 5_000, to: 6_000 }).map((t) => t.label).slice(0, 3)).toEqual([
      "0:05.0", "0:05.1", "0:05.2",
    ]);
  });

  it("is empty without a length", () => {
    expect(rulerTicks(0)).toEqual([]);
    expect(rulerTicks(NaN)).toEqual([]);
  });
});

describe("toggleMute", () => {
  it("mutes a playing track and remembers its level", () => {
    expect(toggleMute(70, null)).toEqual({ volume: 0, remembered: 70 });
  });

  it("unmutes to the remembered level", () => {
    expect(toggleMute(0, 70)).toEqual({ volume: 70, remembered: null });
  });

  it("unmutes to 100 when no level is remembered", () => {
    expect(toggleMute(0, null)).toEqual({ volume: 100, remembered: null });
    expect(toggleMute(0, 0)).toEqual({ volume: 100, remembered: null });
  });
});

describe("laneShading", () => {
  const trim = { start: 10_000, end: 85_000 };

  it("dims outside the trim", () => {
    const { dims } = laneShading(100_000, trim, [{ start: 0, end: 100_000, removed: false }]);
    expect(dims).toEqual([
      { left: 0, width: 10 },
      { left: 85, width: 15 },
    ]);
  });

  it("shows only the part inside a zoomed-in view", () => {
    const pieces = [
      { start: 0, end: 40_000, removed: false },
      { start: 40_000, end: 60_000, removed: true },
      { start: 60_000, end: 100_000, removed: false },
    ];
    const { dims, cuts } = laneShading(100_000, trim, pieces, { from: 50_000, to: 90_000 });
    expect(dims).toEqual([{ left: 87.5, width: 12.5 }]);
    expect(cuts).toEqual([{ left: 0, width: 25 }]);
  });

  it("shades removed parts, only as far as they are inside the trim", () => {
    const pieces = [
      { start: 0, end: 20_000, removed: true },
      { start: 20_000, end: 46_000, removed: false },
      { start: 46_000, end: 62_000, removed: true },
      { start: 62_000, end: 90_000, removed: false },
      { start: 90_000, end: 100_000, removed: true },
    ];
    expect(laneShading(100_000, trim, pieces).cuts).toEqual([
      { left: 10, width: 10 },
      { left: 46, width: 16 },
    ]);
  });

  it("shades nothing without a length", () => {
    expect(laneShading(0, { start: 0, end: 0 }, [{ start: 0, end: 0, removed: true }]).cuts).toEqual([]);
  });
});

describe("blockLayout", () => {
  it("a wide block has 10 px resize edges, its text and its remove button", () => {
    expect(blockLayout(200)).toEqual({ edgePx: 10, showText: true, showRemove: true });
  });
  it("a short block keeps its middle for moving: each edge is a quarter of it", () => {
    expect(blockLayout(24).edgePx).toBe(6);
    expect(blockLayout(8).edgePx).toBe(2);
  });
  it("drops the remove button, then the text, when there's no room", () => {
    expect(blockLayout(50)).toEqual({ edgePx: 10, showText: true, showRemove: false });
    expect(blockLayout(20)).toEqual({ edgePx: 5, showText: false, showRemove: false });
  });
});
