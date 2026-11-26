import { afterEach, describe, expect, it, vi } from "vitest";
import { clockLabel, loadPlayerPrefs, nextSpeed, playerKey, savePlayerPrefs } from "./player";

afterEach(() => vi.unstubAllGlobals());

function memoryStorage() {
  const data = new Map<string, string>();
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
  };
}

describe("nextSpeed", () => {
  it("cycles through the speeds and back", () => {
    expect(nextSpeed(0.5)).toBe(1);
    expect(nextSpeed(1)).toBe(1.5);
    expect(nextSpeed(2)).toBe(0.5);
  });
  it("goes to normal speed from anything else", () => {
    expect(nextSpeed(1.25)).toBe(1);
  });
});

describe("clockLabel", () => {
  it("shows minutes and seconds, and hours when there are some", () => {
    expect(clockLabel(11)).toBe("0:11");
    expect(clockLabel(65)).toBe("1:05");
    expect(clockLabel(3723)).toBe("1:02:03");
  });
  it("shows 0:00 for a time it can't show", () => {
    expect(clockLabel(NaN)).toBe("0:00");
    expect(clockLabel(-3)).toBe("0:00");
    expect(clockLabel(Infinity)).toBe("0:00");
  });
});

describe("playerKey", () => {
  it("maps the player's keys", () => {
    expect(playerKey({ key: " " })).toBe("toggle");
    expect(playerKey({ key: "k" })).toBe("toggle");
    expect(playerKey({ key: "m" })).toBe("mute");
    expect(playerKey({ key: "j" })).toBe("back");
    expect(playerKey({ key: "l" })).toBe("forward");
    expect(playerKey({ key: "F" })).toBe("fullscreen");
    expect(playerKey({ key: "Escape" })).toBe("escape");
  });
  it("leaves shortcuts with modifiers and the arrows alone", () => {
    expect(playerKey({ key: "f", ctrlKey: true })).toBeNull();
    expect(playerKey({ key: "m", altKey: true })).toBeNull();
    expect(playerKey({ key: "ArrowLeft" })).toBeNull();
    expect(playerKey({ key: "x" })).toBeNull();
  });
});

describe("player prefs", () => {
  it("defaults when storage throws", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    });
    expect(loadPlayerPrefs()).toEqual({ volume: 1, muted: false });
    expect(() => savePlayerPrefs({ volume: 0.5, muted: true })).not.toThrow();
  });
  it("defaults when nothing was saved", () => {
    vi.stubGlobal("localStorage", memoryStorage());
    expect(loadPlayerPrefs()).toEqual({ volume: 1, muted: false });
  });
  it("keeps what was saved, with the volume held to 0–1", () => {
    vi.stubGlobal("localStorage", memoryStorage());
    savePlayerPrefs({ volume: 0.4, muted: false });
    expect(loadPlayerPrefs()).toEqual({ volume: 0.4, muted: false });
    savePlayerPrefs({ volume: 7, muted: true });
    expect(loadPlayerPrefs()).toEqual({ volume: 1, muted: true });
  });
  it("ignores something unreadable", () => {
    const storage = memoryStorage();
    storage.setItem("klipt.player", "{nope");
    vi.stubGlobal("localStorage", storage);
    expect(loadPlayerPrefs()).toEqual({ volume: 1, muted: false });
  });
});
