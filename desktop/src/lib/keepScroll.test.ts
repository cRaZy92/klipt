import { describe, expect, it } from "vitest";
import { keepScroll } from "./keepScroll";

describe("keepScroll", () => {
  it("puts the scroll position back when the screen is shown again", () => {
    const node = { scrollTop: 600 };
    const action = keepScroll(node as HTMLElement, true);
    action.update(false);
    node.scrollTop = 0; // the engine clamped it while hidden
    action.update(true);
    expect(node.scrollTop).toBe(600);
  });

  it("leaves the position alone while it stays shown", () => {
    const node = { scrollTop: 100 };
    const action = keepScroll(node as HTMLElement, true);
    node.scrollTop = 250;
    action.update(true);
    expect(node.scrollTop).toBe(250);
  });
});
