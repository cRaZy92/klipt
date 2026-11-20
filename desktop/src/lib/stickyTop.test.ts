import { describe, expect, it } from "vitest";
import { compactAt } from "./stickyTop";

describe("compactAt", () => {
  it("goes compact only once scrolled well down", () => {
    expect(compactAt(20, false)).toBe(false);
    expect(compactAt(60, false)).toBe(true);
  });

  it("stays compact until back near the top", () => {
    expect(compactAt(20, true)).toBe(true);
    expect(compactAt(4, true)).toBe(false);
  });
});
