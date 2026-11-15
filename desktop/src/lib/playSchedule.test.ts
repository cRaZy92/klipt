import { describe, expect, it } from "vitest";
import {
  clipMsAt,
  cutSeekAhead,
  schedule,
  SEEK_LATENCY_START_MS,
  seekLatency,
  videoCorrection,
} from "./playSchedule";

const skip = [
  { start: 0, end: 2000 },
  { start: 3000, end: 5000 },
];

describe("schedule", () => {
  it("plays straight through without cuts", () => {
    expect(schedule(1000, [{ start: 0, end: 5000 }])).toEqual([{ at: 0, from: 1000, to: 5000 }]);
  });
  it("skips removed parts", () => {
    expect(schedule(1500, skip)).toEqual([
      { at: 0, from: 1500, to: 2000 },
      { at: 0.5, from: 3000, to: 5000 },
    ]);
  });
  it("starts at the next kept part from inside a removed one", () => {
    expect(schedule(2500, skip)[0]).toEqual({ at: 0, from: 3000, to: 5000 });
  });
  it("loops from the first kept part", () => {
    expect(schedule(5000, skip)[0].from).toBe(0);
  });
  it("is empty when nothing is kept", () => {
    expect(schedule(0, [])).toEqual([]);
  });
});

describe("clipMsAt", () => {
  it("maps elapsed time across a skip", () => {
    const s = schedule(1500, skip);
    expect(clipMsAt(0.75, s)).toBe(3250);
    expect(clipMsAt(3, s)).toBeNull();
  });
});

describe("videoCorrection", () => {
  const end = 10_000;
  it("leaves small drift alone", () => expect(videoCorrection(1000, 1050, 150, end)).toBeNull());
  it("nudges a video a little behind", () => expect(videoCorrection(1000, 1200, 150, end)).toEqual({ rate: 1.03 }));
  it("nudges a video a little ahead", () => expect(videoCorrection(1200, 1000, 150, end)).toEqual({ rate: 0.97 }));
  it("pushes harder on a video further behind", () => expect(videoCorrection(1000, 1400, 150, end)).toEqual({ rate: 1.1 }));
  it("holds back harder on a video further ahead", () => expect(videoCorrection(1400, 1000, 150, end)).toEqual({ rate: 0.9 }));
  it("seeks ahead of the audio by the seek time on large drift", () =>
    expect(videoCorrection(0, 1000, 300, end)).toEqual({ seekMs: 1300 }));
  it("doesn't seek past the part's end", () => expect(videoCorrection(0, 1000, 300, 1200)).toEqual({ seekMs: 1200 }));
});

describe("seekLatency", () => {
  it("starts at 150 ms", () => expect(SEEK_LATENCY_START_MS).toBe(150));
  it("moves toward each measured seek", () => {
    const slower = seekLatency(150, 650);
    expect(slower).toBeGreaterThan(150);
    expect(slower).toBeLessThan(650);
    expect(seekLatency(150, 50)).toBeLessThan(150);
  });
  it("settles on a steady seek time", () => {
    let avg = SEEK_LATENCY_START_MS;
    for (let i = 0; i < 50; i++) avg = seekLatency(avg, 700);
    expect(avg).toBeCloseTo(700, 0);
  });
  it("stays within 0–1500 ms", () => {
    let avg = SEEK_LATENCY_START_MS;
    for (let i = 0; i < 50; i++) avg = seekLatency(avg, 60_000);
    expect(avg).toBeLessThanOrEqual(1500);
    expect(avg).toBeCloseTo(1500, 0);
    expect(seekLatency(150, -500)).toBeGreaterThanOrEqual(0);
    expect(seekLatency(0, -500)).toBe(0);
  });
});

describe("cutSeekAhead", () => {
  const seg = { at: 0, from: 1000, to: 2000 };
  const next = { at: 1, from: 3000, to: 5000 };
  it("waits while the cut is further off than a seek takes", () => expect(cutSeekAhead(1700, seg, next, 200)).toBeNull());
  it("seeks to the next part's start a seek's time before the cut", () =>
    expect(cutSeekAhead(1800, seg, next, 200)).toBe(3000));
  it("seeks further in when it's late", () => expect(cutSeekAhead(1900, seg, next, 200)).toBe(3100));
  it("doesn't seek past the next part's end", () =>
    expect(cutSeekAhead(1999, seg, { at: 1, from: 3000, to: 3050 }, 200)).toBe(3050));
  it("doesn't seek where the next part follows on", () =>
    expect(cutSeekAhead(1900, seg, { at: 1, from: 2000, to: 3000 }, 200)).toBeNull());
  it("doesn't seek after the last part", () => expect(cutSeekAhead(1900, seg, null, 200)).toBeNull());
});
