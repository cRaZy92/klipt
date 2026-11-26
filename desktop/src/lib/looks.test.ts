import { describe, expect, it } from "vitest";
import { ACCENTS, THEMES } from "@klipt/shared";
import { applyLooks, avatarState } from "./looks";

function fakeRoot() {
  const props = new Map<string, string>();
  return { props, style: { setProperty: (k: string, v: string) => void props.set(k, v) } };
}

describe("looks", () => {
  it("sets the accent and theme tokens", () => {
    const root = fakeRoot();
    applyLooks({ accent: "ocean", theme: "klipt", frame: null, banner: null }, root);
    expect(root.props.get("--accent")).toBe(ACCENTS.find((a) => a.id === "ocean")!.color);
    expect(root.props.get("--bg")).toBe(THEMES.find((t) => t.id === "klipt")!.tokens["--bg"]);
  });

  it("falls back to the defaults without looks", () => {
    const root = fakeRoot();
    applyLooks(null, root);
    expect(root.props.get("--accent")).toBe("#7c5cff");
  });

  it("draws the initial when there is no picture or it failed", () => {
    expect(avatarState("http://x/a.jpg", true)).toBe("initial");
    expect(avatarState(null, false)).toBe("initial");
    expect(avatarState(undefined, false)).toBe("initial");
    expect(avatarState("http://x/a.jpg", false)).toBe("image");
  });
});
