import { describe, expect, it } from "vitest";
import { editorKeyAction, type EditorKey } from "./editorKeys";

function key(k: string, mods: Partial<Omit<EditorKey, "key">> = {}): EditorKey {
  return { key: k, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods };
}

describe("editorKeyAction", () => {
  it("Space plays or pauses, even on a focused button or slider", () => {
    expect(editorKeyAction(key(" "), "control", false)).toEqual({ kind: "play" });
    expect(editorKeyAction(key(" "), "slider", false)).toEqual({ kind: "play" });
    expect(editorKeyAction(key(" "), "none", false)).toEqual({ kind: "play" });
  });

  it("Space in a text field or a dialog is left to it", () => {
    expect(editorKeyAction(key(" "), "text", false)).toBeNull();
    expect(editorKeyAction(key(" "), "dialog", false)).toBeNull();
  });

  it("S splits, but not with Ctrl, Alt or Super, nor in a text field", () => {
    expect(editorKeyAction(key("s"), "none", false)).toEqual({ kind: "split" });
    expect(editorKeyAction(key("S", { shiftKey: true }), "control", false)).toEqual({ kind: "split" });
    expect(editorKeyAction(key("s", { ctrlKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("s", { altKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("s", { metaKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("s"), "text", false)).toBeNull();
  });

  it("I and O start and end the trim at the playhead, but not with modifiers or in a text field", () => {
    expect(editorKeyAction(key("i"), "none", false)).toEqual({ kind: "mark", end: "in" });
    expect(editorKeyAction(key("O", { shiftKey: true }), "control", false)).toEqual({ kind: "mark", end: "out" });
    expect(editorKeyAction(key("i", { ctrlKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("o", { altKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("i"), "text", false)).toBeNull();
    expect(editorKeyAction(key("o"), "none", true)).toBeNull();
  });

  it("T adds a subtitle, but not with Ctrl, Alt or Super, nor in a text field", () => {
    expect(editorKeyAction(key("t"), "none", false)).toEqual({ kind: "subtitle" });
    expect(editorKeyAction(key("T", { shiftKey: true }), "control", false)).toEqual({ kind: "subtitle" });
    expect(editorKeyAction(key("t", { ctrlKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("t", { altKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("t", { metaKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("t"), "text", false)).toBeNull();
  });

  it("B blurs a part, but not with modifiers or in a text field", () => {
    expect(editorKeyAction(key("b"), "none", false)).toEqual({ kind: "blur" });
    expect(editorKeyAction(key("B", { shiftKey: true }), "control", false)).toEqual({ kind: "blur" });
    expect(editorKeyAction(key("b", { ctrlKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("b"), "text", false)).toBeNull();
  });

  it("Z zooms; Ctrl+Z still undoes", () => {
    expect(editorKeyAction(key("z"), "none", false)).toEqual({ kind: "zoom" });
    expect(editorKeyAction(key("Z", { shiftKey: true }), "control", false)).toEqual({ kind: "zoom" });
    expect(editorKeyAction(key("z", { ctrlKey: true }), "none", false)).toEqual({ kind: "undo" });
    expect(editorKeyAction(key("z", { altKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("z", { metaKey: true, altKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("z"), "text", false)).toBeNull();
  });

  it("Delete and Backspace remove, but not in a text field", () => {
    expect(editorKeyAction(key("Delete"), "none", false)).toEqual({ kind: "delete" });
    expect(editorKeyAction(key("Backspace"), "control", false)).toEqual({ kind: "delete" });
    expect(editorKeyAction(key("Backspace"), "text", false)).toBeNull();
    expect(editorKeyAction(key("Delete", { ctrlKey: true }), "none", false)).toBeNull();
  });

  it("the arrows step a frame, or a second with Shift", () => {
    expect(editorKeyAction(key("ArrowLeft"), "none", false)).toEqual({ kind: "step", direction: -1, second: false });
    expect(editorKeyAction(key("ArrowRight", { shiftKey: true }), "none", false)).toEqual({
      kind: "step",
      direction: 1,
      second: true,
    });
    // A button clicked and then let go of (focus released) or focused: the arrows still step.
    expect(editorKeyAction(key("ArrowRight"), "control", false)).toEqual({ kind: "step", direction: 1, second: false });
  });

  it("a slider focused from the keyboard keeps its arrows", () => {
    expect(editorKeyAction(key("ArrowLeft"), "slider", false)).toBeNull();
    expect(editorKeyAction(key("ArrowRight", { shiftKey: true }), "slider", false)).toBeNull();
  });

  it("the arrows with Ctrl or Alt, or in a text field, aren't the editor's", () => {
    expect(editorKeyAction(key("ArrowLeft", { ctrlKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("ArrowLeft", { altKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("ArrowLeft"), "text", false)).toBeNull();
  });

  it("Ctrl+Z undoes; Ctrl+Shift+Z and Ctrl+Y redo, also on a slider", () => {
    expect(editorKeyAction(key("z", { ctrlKey: true }), "none", false)).toEqual({ kind: "undo" });
    expect(editorKeyAction(key("z", { ctrlKey: true }), "slider", false)).toEqual({ kind: "undo" });
    expect(editorKeyAction(key("Z", { ctrlKey: true, shiftKey: true }), "control", false)).toEqual({ kind: "redo" });
    expect(editorKeyAction(key("y", { ctrlKey: true }), "none", false)).toEqual({ kind: "redo" });
    expect(editorKeyAction(key("z", { metaKey: true }), "none", false)).toEqual({ kind: "undo" });
    expect(editorKeyAction(key("y", { ctrlKey: true, shiftKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("z", { ctrlKey: true, altKey: true }), "none", false)).toBeNull();
  });

  it("Ctrl+Z in a text field undoes its typing, not the edit", () => {
    expect(editorKeyAction(key("z", { ctrlKey: true }), "text", false)).toBeNull();
    expect(editorKeyAction(key("z", { ctrlKey: true }), "dialog", false)).toBeNull();
  });

  it("nothing while the upload dialog is open", () => {
    for (const k of [key(" "), key("s"), key("ArrowLeft"), key("z", { ctrlKey: true })]) {
      expect(editorKeyAction(k, "none", true)).toBeNull();
    }
  });

  it("+ and − zoom the timeline, 0 shows the whole clip; not with Ctrl or in a text field", () => {
    expect(editorKeyAction(key("+", { shiftKey: true }), "none", false)).toEqual({ kind: "timelineZoom", direction: 1 });
    expect(editorKeyAction(key("="), "none", false)).toEqual({ kind: "timelineZoom", direction: 1 });
    expect(editorKeyAction(key("-"), "control", false)).toEqual({ kind: "timelineZoom", direction: -1 });
    expect(editorKeyAction(key("0"), "none", false)).toEqual({ kind: "timelineZoom", direction: 0 });
    expect(editorKeyAction(key("-", { ctrlKey: true }), "none", false)).toBeNull();
    expect(editorKeyAction(key("0"), "text", false)).toBeNull();
  });

  it("other keys aren't the editor's", () => {
    expect(editorKeyAction(key("Enter"), "none", false)).toBeNull();
    expect(editorKeyAction(key("a"), "none", false)).toBeNull();
  });
});
