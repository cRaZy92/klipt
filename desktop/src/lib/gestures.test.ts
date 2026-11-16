import { describe, expect, it, vi } from "vitest";
import { Gestures } from "./gestures";

describe("Gestures", () => {
  it("is active from a start until its end, and calls onend once at the end", () => {
    const onend = vi.fn();
    const g = new Gestures<"scrub" | "slider">(onend);
    expect(g.active).toBe(false);
    g.start("scrub");
    expect(g.active).toBe(true);
    expect(onend).not.toHaveBeenCalled();
    g.end("scrub");
    expect(g.active).toBe(false);
    expect(onend).toHaveBeenCalledTimes(1);
  });

  it("overlapping gestures end once the last one ends", () => {
    const onend = vi.fn();
    const g = new Gestures<"scrub" | "slider">(onend);
    g.start("scrub");
    g.start("slider");
    g.end("scrub");
    expect(g.active).toBe(true);
    expect(onend).not.toHaveBeenCalled();
    g.end("slider");
    expect(onend).toHaveBeenCalledTimes(1);
  });

  it("an end without a start does nothing", () => {
    const onend = vi.fn();
    const g = new Gestures<"scrub" | "slider">(onend);
    g.end("slider");
    g.start("scrub");
    g.end("slider");
    expect(g.active).toBe(true);
    expect(onend).not.toHaveBeenCalled();
  });

  it("starting the same gesture twice needs only one end", () => {
    const onend = vi.fn();
    const g = new Gestures<"scrub">(onend);
    g.start("scrub");
    g.start("scrub");
    g.end("scrub");
    expect(g.active).toBe(false);
    expect(onend).toHaveBeenCalledTimes(1);
  });
});
