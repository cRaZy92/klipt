import { describe, expect, it } from "vitest";
import { peaks, poolPeaks, windowPeaks } from "./waveform";

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

describe("poolPeaks", () => {
  it("gives what peaks would over the same samples", () => {
    const samples = Float32Array.from({ length: 1000 }, (_, i) => Math.sin(i * 0.37) * ((i % 13) / 13));
    const fine = peaks([samples], 64);
    expect(Array.from(poolPeaks(fine, 16)!)).toEqual(Array.from(peaks([samples], 16)));
    expect(poolPeaks(fine, 64)).toBe(fine);
  });

  it("is null when the buckets don't divide evenly", () => {
    expect(poolPeaks(new Float32Array(64), 24)).toBeNull();
    expect(poolPeaks(new Float32Array(64), 128)).toBeNull();
  });
});

describe("windowPeaks", () => {
  const p = Float32Array.from([0.1, 0.9, 0.2, 0.3, 0.5, 0.4, 0.8, 0.6]);

  it("takes the loudest bucket under each pixel", () => {
    expect(Array.from(windowPeaks(p, 0, 1, 4))).toEqual(Array.from(peaks([p], 4)));
  });

  it("shows only the part of the clip on screen", () => {
    expect(Array.from(windowPeaks(p, 0.5, 1, 2))).toEqual([0.5, 0.8].map(Math.fround));
  });

  it("repeats a bucket over its pixels when zoomed in past them", () => {
    expect(Array.from(windowPeaks(p, 0.25, 0.5, 4))).toEqual([0.2, 0.2, 0.3, 0.3].map(Math.fround));
  });

  it("is empty with nothing to draw", () => {
    expect(windowPeaks(new Float32Array(0), 0, 1, 10).every((v) => v === 0)).toBe(true);
    expect(windowPeaks(p, 0, 1, 0)).toHaveLength(0);
  });
});
