import { describe, expect, it } from "vitest";
import { edgeSpeed } from "./edgeScroll";

describe("edgeSpeed", () => {
  it("is still away from the edges", () => {
    expect(edgeSpeed(500, 1000)).toBe(0);
    expect(edgeSpeed(32, 1000)).toBe(0);
    expect(edgeSpeed(968, 1000)).toBe(0);
  });

  it("moves towards the edge the pointer is near, faster the closer", () => {
    expect(edgeSpeed(1000, 1000)).toBe(0.5);
    expect(edgeSpeed(984, 1000)).toBe(0.25);
    expect(edgeSpeed(0, 1000)).toBe(-0.5);
  });

  it("goes faster past the edge, up to 2 screens a second", () => {
    expect(edgeSpeed(1064, 1000)).toBe(1.5);
    expect(edgeSpeed(5000, 1000)).toBe(2);
    expect(edgeSpeed(-5000, 1000)).toBe(-2);
  });

  it("keeps the zones apart on a narrow area", () => {
    expect(edgeSpeed(50, 100)).toBe(0);
    expect(edgeSpeed(100, 100)).toBe(0.5);
  });

  it("is still without a width", () => {
    expect(edgeSpeed(10, 0)).toBe(0);
    expect(edgeSpeed(NaN, 100)).toBe(0);
  });
});
