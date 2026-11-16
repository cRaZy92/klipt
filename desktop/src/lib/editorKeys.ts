// The editor's keyboard shortcuts: which one a key press is, given what has focus.

/**
 * What has focus, as far as the shortcuts care: a text field (types, and undoes its own typing),
 * a slider (moves with the arrows), something in a dialog, another control (a button, a switch)
 * or nothing in particular.
 */
export type KeyTarget = "text" | "slider" | "dialog" | "control" | "none";

/** The parts of a KeyboardEvent the shortcuts read. */
export interface EditorKey {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

export type EditorKeyAction =
  | { kind: "play" }
  | { kind: "split" }
  | { kind: "undo" }
  | { kind: "redo" }
  /** Moves the playhead one frame, or one second. */
  | { kind: "step"; direction: -1 | 1; second: boolean };

/**
 * The editor's action for a key press on `target`, or null when the key is left to the page.
 * Space plays even on a focused button (its press is prevented); a slider keeps its arrows.
 * Nothing while `dialogOpen` (the upload dialog).
 */
export function editorKeyAction(e: EditorKey, target: KeyTarget, dialogOpen: boolean): EditorKeyAction | null {
  if (dialogOpen || target === "text" || target === "dialog") return null;
  const ctrl = (e.ctrlKey || e.metaKey) && !e.altKey;
  const key = e.key.toLowerCase();
  if (ctrl && key === "z") return { kind: e.shiftKey ? "redo" : "undo" };
  if (ctrl && !e.shiftKey && key === "y") return { kind: "redo" };
  if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && !ctrl && !e.altKey) {
    if (target === "slider") return null;
    return { kind: "step", direction: e.key === "ArrowLeft" ? -1 : 1, second: e.shiftKey };
  }
  if (e.key === " ") return { kind: "play" };
  if (key === "s" && !e.ctrlKey && !e.metaKey && !e.altKey) return { kind: "split" };
  return null;
}

/** Sliders: the volume ranges and the timeline's handles. */
const SLIDER = "input[type='range'], [role='slider']";
/** Text fields: any input but a range, checkbox, radio or button. */
const TEXT = "input:not([type='range'], [type='checkbox'], [type='radio'], [type='button']), textarea, select, [contenteditable]";

/** What `el` (a key press's target) is, for `editorKeyAction`. */
export function keyTarget(el: Element | null): KeyTarget {
  if (!el) return "none";
  if (el.closest("[role='dialog']")) return "dialog";
  if (el.closest(TEXT)) return "text";
  if (el.closest(SLIDER)) return "slider";
  if (el.closest("button, input, a[href]")) return "control";
  return "none";
}

/**
 * A control that keeps the keyboard after a click or drag: once the pointer lets go of it, focus
 * is released so Space and the arrows reach the editor again.
 */
export const RELEASES_FOCUS = `button, ${SLIDER}`;
