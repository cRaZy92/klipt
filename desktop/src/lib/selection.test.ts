import { describe, expect, it } from "vitest";
import { keepShown, selectClick } from "./selection";

const order = ["a", "b", "c", "d", "e"];

describe("selectClick", () => {
  it("toggles one card on a plain click", () => {
    const one = selectClick(new Set(), order, "b", null, false);
    expect([...one]).toEqual(["b"]);
    expect([...selectClick(one, order, "b", "b", false)]).toEqual([]);
  });

  it("selects the run from the anchor on Shift+click, either direction", () => {
    expect([...selectClick(new Set(["b"]), order, "d", "b", true)].sort()).toEqual(["b", "c", "d"]);
    expect([...selectClick(new Set(["d"]), order, "b", "d", true)].sort()).toEqual(["b", "c", "d"]);
  });

  it("keeps what was selected outside the run", () => {
    expect([...selectClick(new Set(["a", "d"]), order, "e", "d", true)].sort()).toEqual(["a", "d", "e"]);
  });

  it("clears the run when the anchor was just unselected", () => {
    expect([...selectClick(new Set(["a", "c", "d"]), order, "d", "b", true)].sort()).toEqual(["a"]);
  });

  it("falls back to a toggle when the anchor is gone", () => {
    expect([...selectClick(new Set(), order, "c", "zz", true)]).toEqual(["c"]);
    expect([...selectClick(new Set(), order, "c", null, true)]).toEqual(["c"]);
  });
});

describe("keepShown", () => {
  it("drops hidden keys", () => {
    expect([...keepShown(new Set(["a", "x"]), order)]).toEqual(["a"]);
  });

  it("returns the same set when nothing is hidden", () => {
    const set = new Set(["a", "b"]);
    expect(keepShown(set, order)).toBe(set);
  });
});
