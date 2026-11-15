import { describe, expect, it } from "vitest";
import { clipMsAt, schedule, videoCorrection } from "./playSchedule";

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
  it("leaves small drift alone", () => expect(videoCorrection(1000, 1050)).toBeNull());
  it("speeds up a video that is behind", () => expect(videoCorrection(1000, 1200)).toEqual({ rate: 1.03 }));
  it("slows down a video that is ahead", () => expect(videoCorrection(1200, 1000)).toEqual({ rate: 0.97 }));
  it("seeks on large drift", () => expect(videoCorrection(0, 1000)).toEqual({ seekMs: 1000 }));
});
