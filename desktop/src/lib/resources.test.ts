import { describe, expect, it } from "vitest";
import type { ResourceUsage } from "./api";
import { formatPercent, pushSample, scaleTop, sparkArea, sparkPath, totals } from "./resources";

const usage = (gpu: boolean): ResourceUsage => ({
  parts: [
    { part: "app", processes: 1, ramBytes: 100, cpuPercent: 1, gpuPercent: gpu ? 0 : null, gpuEngine: null, vramBytes: gpu ? 0 : null },
    { part: "recorder", processes: 1, ramBytes: 300, cpuPercent: 2.5, gpuPercent: gpu ? 7 : null, gpuEngine: gpu ? "encoder" : null, vramBytes: gpu ? 50 : null },
  ],
  totalRamBytes: 1000,
  cpuThreads: 16,
  gpuAvailable: gpu,
  gpuNote: gpu ? null : "none",
  gpuName: null,
  totalVramBytes: null,
});

describe("totals", () => {
  it("adds up every part", () => {
    expect(totals(usage(true))).toEqual({ ram: 400, cpu: 3.5, gpu: 7, vram: 50 });
  });
  it("leaves GPU numbers out when they can't be read", () => {
    expect(totals(usage(false))).toEqual({ ram: 400, cpu: 3.5, gpu: null, vram: null });
  });
});

describe("pushSample", () => {
  it("keeps the newest", () => {
    expect(pushSample([1, 2, 3], 4, 3)).toEqual([2, 3, 4]);
    expect(pushSample([], 1, 3)).toEqual([1]);
  });
});

describe("scaleTop", () => {
  it("leaves room above the highest value", () => {
    expect(scaleTop([10, 40, null], 5)).toBe(50);
  });
  it("never goes under the floor", () => {
    expect(scaleTop([0.2, null], 5)).toBe(5);
    expect(scaleTop([], 5)).toBe(5);
  });
});

describe("sparkPath", () => {
  it("puts the newest at the right edge", () => {
    expect(sparkPath([0, 10], 10, 100, 20, 3)).toBe("M50 20L100 0");
  });
  it("breaks the line at a missing value", () => {
    expect(sparkPath([5, null, 5], 10, 100, 20, 3)).toBe("M0 10M100 10");
  });
  it("clips values over the top", () => {
    expect(sparkPath([30], 10, 100, 20, 1)).toBe("M0 0");
  });
});

describe("sparkArea", () => {
  it("closes each run down to the bottom", () => {
    expect(sparkArea([0, 10], 10, 100, 20, 3)).toBe("M50 20L100 0L100 20L50 20Z");
  });
});

describe("formatPercent", () => {
  it("shows a decimal under 10", () => {
    expect(formatPercent(0)).toBe("0%");
    expect(formatPercent(0.01)).toBe("0.1%");
    expect(formatPercent(0.64)).toBe("0.6%");
    expect(formatPercent(4)).toBe("4%");
    expect(formatPercent(12.6)).toBe("13%");
  });
});
