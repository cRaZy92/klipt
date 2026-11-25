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
  /** Adds a zoom at the playhead. */
  | { kind: "zoom" }
  /** Adds a subtitle at the playhead. */
  | { kind: "subtitle" }
  /** Blurs a part of the picture from the playhead. */
  | { kind: "blur" }
  /** Starts (I) or ends (O) the trim at the playhead. */
  | { kind: "mark"; end: "in" | "out" }
  /** Removes what is picked (the picked zoom, blurred part, subtitle or muted part). */
  | { kind: "delete" }
  | { kind: "undo" }
  | { kind: "redo" }
  /** Moves the playhead one frame, or one second. */
  | { kind: "step"; direction: -1 | 1; second: boolean }
  /** Zooms the timeline in (1), out (-1) or back to the whole clip (0). */
  | { kind: "timelineZoom"; direction: -1 | 0 | 1 };

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
  if (key === "z" && !e.ctrlKey && !e.metaKey && !e.altKey) return { kind: "zoom" };
  if (key === "t" && !e.ctrlKey && !e.metaKey && !e.altKey) return { kind: "subtitle" };
  if (key === "b" && !e.ctrlKey && !e.metaKey && !e.altKey) return { kind: "blur" };
  const plain = !e.ctrlKey && !e.metaKey && !e.altKey;
  if (plain && (key === "i" || key === "o")) return { kind: "mark", end: key === "i" ? "in" : "out" };
  if (plain && (e.key === "+" || e.key === "=")) return { kind: "timelineZoom", direction: 1 };
  if (plain && (e.key === "-" || e.key === "_")) return { kind: "timelineZoom", direction: -1 };
  if (plain && e.key === "0") return { kind: "timelineZoom", direction: 0 };
  if ((e.key === "Delete" || e.key === "Backspace") && !ctrl && !e.altKey && target !== "slider") return { kind: "delete" };
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
