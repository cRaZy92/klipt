import { describe, expect, it } from "vitest";
import { formatElapsed } from "./elapsed";

describe("formatElapsed", () => {
  it("shows minutes and seconds, and hours once there are any", () => {
    expect(formatElapsed(7_000)).toBe("0:07");
    expect(formatElapsed(754_000)).toBe("12:34");
    expect(formatElapsed(3_723_000)).toBe("1:02:03");
    expect(formatElapsed(-5)).toBe("0:00");
  });
});
