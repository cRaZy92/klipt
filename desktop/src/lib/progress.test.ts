import { describe, expect, it } from "vitest";
import { editorStep, formatRate, RateMeter, timeLeft, transferDetail } from "./progress";

const MB = 1024 * 1024;

describe("RateMeter", () => {
  it("has no rate until it has seen a second of progress", () => {
    const m = new RateMeter();
    m.add(0, 0);
    m.add(500, 2 * MB);
    expect(m.rate()).toBeNull();
    m.add(1000, 4 * MB);
    expect(m.rate()).toBeCloseTo(4 * MB);
  });

  it("averages over the last few seconds only", () => {
    const m = new RateMeter(3000);
    m.add(0, 0);
    m.add(1000, 10 * MB); // a burst at the start
    for (let t = 2000; t <= 6000; t += 1000) m.add(t, 10 * MB + ((t - 1000) / 1000) * MB);
    expect(m.rate()).toBeCloseTo(MB);
  });

  it("starts over when the count goes back (a resend after a failure)", () => {
    const m = new RateMeter();
    m.add(0, 0);
    m.add(2000, 8 * MB);
    m.add(2100, 4 * MB);
    expect(m.rate()).toBeNull();
    m.add(3100, 6 * MB);
    expect(m.rate()).toBeCloseTo(2 * MB);
  });
});

describe("formatRate", () => {
  it("shows MB/s, or KB/s when slow", () => {
    expect(formatRate(6.24 * MB)).toBe("6.2 MB/s");
    expect(formatRate(12.6 * MB)).toBe("13 MB/s");
    expect(formatRate(300 * 1024)).toBe("300 KB/s");
  });
});

describe("timeLeft", () => {
  it("rounds to seconds, then minutes", () => {
    expect(timeLeft(4.2)).toBe("5 s left");
    expect(timeLeft(0.2)).toBe("1 s left");
    expect(timeLeft(59)).toBe("59 s left");
    expect(timeLeft(150)).toBe("3 min left");
  });
});

describe("transferDetail", () => {
  it("shows size, speed and time left once the speed is known", () => {
    expect(transferDetail(12 * MB, 38 * MB, 2 * MB)).toBe("12 MB of 38 MB · 2 MB/s · 13 s left");
  });

  it("shows only the size before then", () => {
    expect(transferDetail(0, 38 * MB, null)).toBe("0 MB of 38 MB");
  });
});

describe("editorStep", () => {
  it("reads the file first", () => {
    expect(editorStep(null)).toEqual({ label: "Opening the clip…", fraction: null });
    expect(editorStep({ step: "reading", track: 0, tracks: 0, fraction: 0 })).toEqual({
      label: "Reading the clip…",
      fraction: null,
    });
  });

  it("counts audio tracks into one bar", () => {
    expect(editorStep({ step: "audio", track: 1, tracks: 2, fraction: 0.5 })).toEqual({
      label: "Extracting audio (track 2 of 2)…",
      fraction: 0.75,
    });
    expect(editorStep({ step: "audio", track: 0, tracks: 1, fraction: 0.4 })).toEqual({
      label: "Extracting audio…",
      fraction: 0.4,
    });
  });
});
