import { describe, expect, it } from "vitest";
import { navKey, neighbors } from "./clipNav";

const list = [{ id: "a" }, { id: "b" }, { id: "c" }];

describe("neighbors", () => {
  it("gives the clips before and after in list order", () => {
    expect(neighbors(list, "b")).toEqual({ prev: list[0], next: list[2] });
    expect(neighbors(list, "a")).toEqual({ prev: null, next: list[1] });
    expect(neighbors(list, "c")).toEqual({ prev: list[1], next: null });
  });

  it("has no neighbours for a clip that isn't in the list", () => {
    expect(neighbors(list, "zz")).toEqual({ prev: null, next: null });
    expect(neighbors([], "a")).toEqual({ prev: null, next: null });
  });
});

describe("navKey", () => {
  const key = (k: string, mods: Partial<KeyboardEvent> = {}) => ({ key: k, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...mods });

  it("maps the left and right arrows to previous and next", () => {
    expect(navKey(key("ArrowRight"))).toBe("next");
    expect(navKey(key("ArrowLeft"))).toBe("prev");
  });

  it("ignores other keys and arrows with a modifier", () => {
    expect(navKey(key("ArrowUp"))).toBeNull();
    expect(navKey(key(" "))).toBeNull();
    expect(navKey(key("ArrowRight", { altKey: true }))).toBeNull();
    expect(navKey(key("ArrowLeft", { ctrlKey: true }))).toBeNull();
    expect(navKey(key("ArrowLeft", { shiftKey: true }))).toBeNull();
  });
});
