// The "press the new shortcut" field: turns a keydown into Klipt's canonical accelerator
// (modifiers Ctrl, Alt, Shift, Super, then one key named by its position, W3C `code`), the
// vocabulary `hotkey::normalize` in the core accepts.

/** The parts of a KeyboardEvent the mapping reads. */
export interface KeyInput {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

export type KeyResult =
  | { kind: "accel"; accel: string }
  /** Only modifiers are down so far: keep waiting. */
  | { kind: "modifiersOnly" }
  /** A key without Ctrl, Alt, Shift or Super. */
  | { kind: "needsModifier" }
  /** A key Klipt can't use as a shortcut. */
  | { kind: "unsupported" }
  /** Escape on its own: stop capturing. */
  | { kind: "cancel" };

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "AltGraph",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

/** Named keys `normalize` accepts (its NAMED_KEYS), by W3C code. */
const NAMED_CODES = new Set([
  "Backquote",
  "Minus",
  "Equal",
  "BracketLeft",
  "BracketRight",
  "Backslash",
  "Semicolon",
  "Quote",
  "Comma",
  "Period",
  "Slash",
  "Space",
  "Insert",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "Pause",
  "PrintScreen",
]);

/** The canonical key name for a W3C code, or null. */
function keyName(code: string): string | null {
  let m = /^Key([A-Z])$/.exec(code);
  if (m) return m[1];
  m = /^Digit([0-9])$/.exec(code);
  if (m) return m[1];
  m = /^F([1-9][0-9]?)$/.exec(code);
  if (m && Number(m[1]) <= 24) return code;
  return NAMED_CODES.has(code) ? code : null;
}

export function acceleratorFromKeydown(e: KeyInput): KeyResult {
  if (MODIFIER_CODES.has(e.code)) return { kind: "modifiersOnly" };
  const mods = [
    e.ctrlKey && "Ctrl",
    e.altKey && "Alt",
    e.shiftKey && "Shift",
    e.metaKey && "Super",
  ].filter((m): m is string => !!m);
  if (e.code === "Escape" && mods.length === 0) return { kind: "cancel" };
  const key = keyName(e.code);
  if (!key) return { kind: "unsupported" };
  if (mods.length === 0) return { kind: "needsModifier" };
  return { kind: "accel", accel: [...mods, key].join("+") };
}
