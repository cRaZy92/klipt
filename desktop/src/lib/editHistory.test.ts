import { describe, expect, it } from "vitest";
import { EditHistory } from "./editHistory";

function setup(initial: { v: number } = { v: 0 }) {
  let t = 1000;
  const h = new EditHistory(initial, 400, () => t);
  return { h, advance: (ms: number) => (t += ms) };
}

describe("EditHistory", () => {
  it("undo returns the previous state", () => {
    const { h, advance } = setup();
    advance(1000);
    h.record({ v: 1 });
    expect(h.canUndo).toBe(true);
    expect(h.undo()).toEqual({ v: 0 });
    expect(h.canUndo).toBe(false);
  });

  it("undo at the start returns null", () => {
    const { h } = setup();
    expect(h.undo()).toBeNull();
    expect(h.canUndo).toBe(false);
  });

  it("a burst of records within the window is one undo step", () => {
    const { h, advance } = setup();
    advance(1000);
    h.record({ v: 1 });
    advance(100);
    h.record({ v: 2 });
    advance(399);
    h.record({ v: 3 });
    expect(h.undo()).toEqual({ v: 0 });
    expect(h.undo()).toBeNull();
    expect(h.redo()).toEqual({ v: 3 });
  });

  it("records further apart than the window are separate steps", () => {
    const { h, advance } = setup();
    advance(1000);
    h.record({ v: 1 });
    advance(401);
    h.record({ v: 2 });
    expect(h.undo()).toEqual({ v: 1 });
    expect(h.undo()).toEqual({ v: 0 });
  });

  it("the first record never overwrites the initial state", () => {
    const { h } = setup();
    h.record({ v: 1 });
    expect(h.undo()).toEqual({ v: 0 });
  });

  it("redo after undo returns the undone state", () => {
    const { h, advance } = setup();
    advance(1000);
    h.record({ v: 1 });
    h.undo();
    expect(h.canRedo).toBe(true);
    expect(h.redo()).toEqual({ v: 1 });
    expect(h.canRedo).toBe(false);
    expect(h.redo()).toBeNull();
  });

  it("record after undo clears redo", () => {
    const { h, advance } = setup();
    advance(1000);
    h.record({ v: 1 });
    h.undo();
    advance(1000);
    h.record({ v: 2 });
    expect(h.canRedo).toBe(false);
    expect(h.redo()).toBeNull();
    expect(h.undo()).toEqual({ v: 0 });
  });

  it("a record right after an undo starts a new step", () => {
    const { h, advance } = setup();
    advance(1000);
    h.record({ v: 1 });
    advance(1000);
    h.record({ v: 2 });
    h.undo();
    h.record({ v: 3 });
    expect(h.undo()).toEqual({ v: 1 });
  });

  it("equal states are not recorded", () => {
    const { h, advance } = setup();
    h.record({ v: 0 });
    expect(h.canUndo).toBe(false);
    advance(1000);
    h.record({ v: 1 });
    advance(1000);
    h.record({ v: 1 });
    expect(h.undo()).toEqual({ v: 0 });
    expect(h.undo()).toBeNull();
  });

  it("stores copies so later mutation cannot change history", () => {
    const initial = { v: 0 };
    const { h, advance } = setup(initial);
    const s = { v: 1 };
    advance(1000);
    h.record(s);
    s.v = 99;
    initial.v = 98;
    expect(h.undo()).toEqual({ v: 0 });
    const redone = h.redo()!;
    expect(redone).toEqual({ v: 1 });
    redone.v = 77;
    h.undo();
    expect(h.redo()).toEqual({ v: 1 });
  });
});
