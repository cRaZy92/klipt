import { describe, expect, it } from "vitest";
import { menuTarget, trapTarget } from "./a11y";

describe("menuTarget", () => {
  it("moves down and up, wrapping at the ends", () => {
    expect(menuTarget("ArrowDown", 0, 3)).toBe(1);
    expect(menuTarget("ArrowDown", 2, 3)).toBe(0);
    expect(menuTarget("ArrowUp", 0, 3)).toBe(2);
    expect(menuTarget("ArrowUp", 2, 3)).toBe(1);
  });

  it("starts from the matching end when nothing is focused", () => {
    expect(menuTarget("ArrowDown", -1, 3)).toBe(0);
    expect(menuTarget("ArrowUp", -1, 3)).toBe(2);
  });

  it("jumps with Home and End and ignores other keys", () => {
    expect(menuTarget("Home", 2, 3)).toBe(0);
    expect(menuTarget("End", 0, 3)).toBe(2);
    expect(menuTarget("a", 0, 3)).toBeNull();
    expect(menuTarget("ArrowDown", -1, 0)).toBeNull();
  });
});

describe("trapTarget", () => {
  it("wraps Tab from the last element to the first, and Shift+Tab back", () => {
    expect(trapTarget(false, 2, 3)).toBe(0);
    expect(trapTarget(true, 0, 3)).toBe(2);
  });

  it("lets the browser move focus in the middle", () => {
    expect(trapTarget(false, 0, 3)).toBeNull();
    expect(trapTarget(true, 2, 3)).toBeNull();
  });

  it("pulls focus back in when it is outside", () => {
    expect(trapTarget(false, -1, 3)).toBe(0);
    expect(trapTarget(true, -1, 3)).toBe(2);
    expect(trapTarget(false, -1, 0)).toBeNull();
  });
});
