import { describe, expect, it } from "vitest";
import { acceleratorFromKeydown, type KeyInput } from "./hotkey";

const key = (code: string, mods: Partial<Omit<KeyInput, "code">> = {}): KeyInput => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("acceleratorFromKeydown", () => {
  it("turns the default shortcut into Alt+Backquote by key position", () => {
    // On a Slovak layout this key types ";", but its code is still Backquote.
    expect(acceleratorFromKeydown(key("Backquote", { altKey: true }))).toEqual({ kind: "accel", accel: "Alt+Backquote" });
  });

  it("uses the canonical modifier order and key names", () => {
    expect(acceleratorFromKeydown(key("KeyS", { shiftKey: true, ctrlKey: true }))).toEqual({
      kind: "accel",
      accel: "Ctrl+Shift+S",
    });
    expect(acceleratorFromKeydown(key("Digit5", { metaKey: true, altKey: true }))).toEqual({
      kind: "accel",
      accel: "Alt+Super+5",
    });
    expect(acceleratorFromKeydown(key("F9", { altKey: true }))).toEqual({ kind: "accel", accel: "Alt+F9" });
    expect(acceleratorFromKeydown(key("F24", { ctrlKey: true }))).toEqual({ kind: "accel", accel: "Ctrl+F24" });
    expect(acceleratorFromKeydown(key("PageDown", { ctrlKey: true }))).toEqual({ kind: "accel", accel: "Ctrl+PageDown" });
    expect(acceleratorFromKeydown(key("Semicolon", { altKey: true }))).toEqual({ kind: "accel", accel: "Alt+Semicolon" });
    expect(acceleratorFromKeydown(key("Space", { ctrlKey: true }))).toEqual({ kind: "accel", accel: "Ctrl+Space" });
    expect(acceleratorFromKeydown(key("PrintScreen", { altKey: true }))).toEqual({
      kind: "accel",
      accel: "Alt+PrintScreen",
    });
  });

  it("waits while only modifiers are held", () => {
    for (const code of ["AltLeft", "AltRight", "ControlLeft", "ShiftRight", "MetaLeft", "OSRight"]) {
      expect(acceleratorFromKeydown(key(code, { altKey: true }))).toEqual({ kind: "modifiersOnly" });
    }
  });

  it("requires at least one modifier", () => {
    expect(acceleratorFromKeydown(key("KeyA"))).toEqual({ kind: "needsModifier" });
    expect(acceleratorFromKeydown(key("F9"))).toEqual({ kind: "needsModifier" });
  });

  it("cancels on a bare Escape", () => {
    expect(acceleratorFromKeydown(key("Escape"))).toEqual({ kind: "cancel" });
  });

  it("rejects keys Klipt can't bind", () => {
    expect(acceleratorFromKeydown(key("Enter", { altKey: true }))).toEqual({ kind: "unsupported" });
    expect(acceleratorFromKeydown(key("Numpad1", { altKey: true }))).toEqual({ kind: "unsupported" });
    expect(acceleratorFromKeydown(key("F25", { altKey: true }))).toEqual({ kind: "unsupported" });
    expect(acceleratorFromKeydown(key("", { altKey: true }))).toEqual({ kind: "unsupported" });
    expect(acceleratorFromKeydown(key("Escape", { altKey: true }))).toEqual({ kind: "unsupported" });
  });
});
