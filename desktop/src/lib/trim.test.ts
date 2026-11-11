import { describe, expect, it } from "vitest";
import { clampThumb, clampTrim, defaultThumb, formatClock, msToX, savedTrim, xToMs } from "./trim";

describe("clampTrim", () => {
  it("keeps a valid range as it is", () => {
    expect(clampTrim(1_000, 5_000, 10_000)).toEqual({ start: 1_000, end: 5_000 });
    expect(clampTrim(0, 10_000, 10_000)).toEqual({ start: 0, end: 10_000 });
  });

  it("clamps both ends into the clip", () => {
    expect(clampTrim(-300, 12_000, 10_000)).toEqual({ start: 0, end: 10_000 });
  });

  it("keeps at least the minimum length, moving the end by default", () => {
    expect(clampTrim(4_000, 4_200, 10_000)).toEqual({ start: 4_000, end: 4_500 });
    expect(clampTrim(6_000, 2_000, 10_000)).toEqual({ start: 6_000, end: 6_500 });
    expect(clampTrim(4_000, 4_200, 10_000, 1_000)).toEqual({ start: 4_000, end: 5_000 });
  });

  it("moves the start instead when the end is the handle being kept", () => {
    expect(clampTrim(4_000, 4_200, 10_000, 500, "end")).toEqual({ start: 3_700, end: 4_200 });
    expect(clampTrim(9_000, 100, 10_000, 500, "end")).toEqual({ start: 0, end: 500 });
  });

  it("never pushes past the clip's edges to make room", () => {
    expect(clampTrim(9_900, 10_000, 10_000)).toEqual({ start: 9_500, end: 10_000 });
    expect(clampTrim(0, 100, 10_000, 500, "end")).toEqual({ start: 0, end: 500 });
  });

  it("keeps 0 <= start < end <= duration for any input", () => {
    for (const [s, e] of [
      [0, 0],
      [10_000, 10_000],
      [5_000, 5_000],
      [-1, -1],
      [20_000, 30_000],
    ]) {
      for (const keep of ["start", "end"] as const) {
        const t = clampTrim(s, e, 10_000, 500, keep);
        expect(t.start).toBeGreaterThanOrEqual(0);
        expect(t.end).toBeLessThanOrEqual(10_000);
        expect(t.end - t.start).toBeGreaterThanOrEqual(500);
      }
    }
  });

  it("uses the whole clip when it is shorter than the minimum length", () => {
    expect(clampTrim(100, 200, 300)).toEqual({ start: 0, end: 300 });
  });

  it("rounds to whole milliseconds and survives bad numbers", () => {
    expect(clampTrim(1_000.4, 5_000.6, 10_000)).toEqual({ start: 1_000, end: 5_001 });
    expect(clampTrim(Number.NaN, Number.NaN, 10_000)).toEqual({ start: 0, end: 10_000 });
    expect(clampTrim(0, 1_000, 0)).toEqual({ start: 0, end: 0 });
  });
});

describe("msToX / xToMs", () => {
  it("maps time to position proportionally", () => {
    expect(msToX(5_000, 10_000, 800)).toBe(400);
    expect(msToX(0, 10_000, 800)).toBe(0);
    expect(msToX(10_000, 10_000, 800)).toBe(800);
    expect(xToMs(200, 10_000, 800)).toBe(2_500);
  });

  it("clamps to the bar", () => {
    expect(msToX(-50, 10_000, 800)).toBe(0);
    expect(msToX(20_000, 10_000, 800)).toBe(800);
    expect(xToMs(-10, 10_000, 800)).toBe(0);
    expect(xToMs(900, 10_000, 800)).toBe(10_000);
  });

  it("rounds xToMs to whole milliseconds", () => {
    expect(xToMs(1, 10_000, 3)).toBe(3_333);
  });

  it("round-trips", () => {
    expect(xToMs(msToX(1_234, 10_000, 640), 10_000, 640)).toBe(1_234);
  });

  it("is safe with an empty clip or a bar that isn't laid out yet", () => {
    expect(msToX(100, 0, 800)).toBe(0);
    expect(xToMs(100, 10_000, 0)).toBe(0);
    expect(msToX(Number.NaN, 10_000, 800)).toBe(0);
  });
});

describe("savedTrim", () => {
  it("stores no trim for an untouched end", () => {
    expect(savedTrim({ start: 0, end: 10_000 }, 10_000)).toEqual({ start: null, end: null });
    expect(savedTrim({ start: 1_000, end: 10_000 }, 10_000)).toEqual({ start: 1_000, end: null });
    expect(savedTrim({ start: 0, end: 4_000 }, 10_000)).toEqual({ start: null, end: 4_000 });
    expect(savedTrim({ start: 1_000, end: 4_000 }, 10_000)).toEqual({ start: 1_000, end: 4_000 });
  });
});

describe("formatClock", () => {
  it("shows minutes, seconds and tenths", () => {
    expect(formatClock(0)).toBe("0:00.0");
    expect(formatClock(5_340)).toBe("0:05.3");
    expect(formatClock(65_000)).toBe("1:05.0");
    expect(formatClock(59_960)).toBe("0:59.9");
  });

  it("treats bad input as zero", () => {
    expect(formatClock(-1)).toBe("0:00.0");
    expect(formatClock(Number.NaN)).toBe("0:00.0");
  });
});

describe("thumbnail frame", () => {
  it("defaults to 1 s into the cut, or its middle when shorter", () => {
    expect(defaultThumb({ start: 4000, end: 20000 })).toBe(5000);
    expect(defaultThumb({ start: 0, end: 1200 })).toBe(600);
  });

  it("stays inside the cut", () => {
    const cut = { start: 4000, end: 10000 };
    expect(clampThumb(6000, cut)).toBe(6000);
    expect(clampThumb(1000, cut)).toBe(4000);
    expect(clampThumb(12000, cut)).toBe(9999);
    expect(clampThumb(6000.6, cut)).toBe(6001);
  });
});
