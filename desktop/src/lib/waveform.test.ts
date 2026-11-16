import { describe, expect, it } from "vitest";
import { peaks } from "./waveform";

describe("peaks", () => {
  it("takes the max absolute sample per bucket", () => {
    expect(Array.from(peaks([new Float32Array([0, 0.5, -1, 0.25])], 2))).toEqual([0.5, 1]);
  });

  it("takes the max over all channels", () => {
    const a = new Float32Array([0.1, 0.9]);
    const b = new Float32Array([-0.4, 0.2]);
    const out = peaks([a, b], 2);
    expect(out[0]).toBeCloseTo(0.4);
    expect(out[1]).toBeCloseTo(0.9);
  });

  it("is empty without channels, samples or buckets", () => {
    expect(peaks([], 4)).toHaveLength(0);
    expect(peaks([new Float32Array([1])], 0)).toHaveLength(0);
    expect(peaks([new Float32Array(0)], 4)).toHaveLength(0);
  });

  it("still returns every bucket when there are more buckets than samples", () => {
    const out = peaks([new Float32Array([0.5, 1])], 4);
    expect(out).toHaveLength(4);
    expect(Array.from(out).filter((v) => v === 0).length).toBeGreaterThan(0);
    expect(Math.max(...out)).toBe(1);
  });
});
