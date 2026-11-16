import { describe, expect, it } from "vitest";
import { formatTenths, stepMs } from "./timecode";

describe("formatTenths", () => {
  it("shows minutes, seconds and tenths", () => {
    expect(formatTenths(12_440)).toBe("0:12.4");
    expect(formatTenths(61_000)).toBe("1:01.0");
    expect(formatTenths(0)).toBe("0:00.0");
  });

  it("rounds down to the tenth", () => {
    expect(formatTenths(12_999)).toBe("0:12.9");
  });

  it("shows a negative or invalid time as zero", () => {
    expect(formatTenths(-500)).toBe("0:00.0");
    expect(formatTenths(Number.NaN)).toBe("0:00.0");
  });
});

describe("stepMs", () => {
  it("is one frame at the clip's frame rate", () => {
    expect(stepMs(50)).toBe(20);
    expect(stepMs(25)).toBe(40);
  });

  it("assumes 60 fps when the rate is unknown", () => {
    expect(stepMs(0)).toBeCloseTo(1000 / 60);
    expect(stepMs(-1)).toBeCloseTo(1000 / 60);
    expect(stepMs(Number.NaN)).toBeCloseTo(1000 / 60);
  });
});
