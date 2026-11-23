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

import { recordingLabel } from "./elapsed";

describe("recordingLabel", () => {
  it("adds the size when known", () => {
    const label = recordingLabel(724_000, 3.1 * 1024 ** 3);
    expect(label).toContain("12:04");
    expect(label).toContain("3.1 GB");
    expect(label).toContain("·");
  });

  it("is just the time while the size is unknown", () => {
    expect(recordingLabel(5_000, null)).toBe("0:05");
  });
});
