import { describe, expect, it } from "vitest";
import type { Settings } from "./api";
import { bitrateKbps, estimateRamMb, parseAppList, sameSettings, validateSettings } from "./settings";

const defaults: Settings = {
  recordingMode: "always",
  bufferLocation: "ram",
  displayId: null,
  resolution: "p1080",
  fps: 60,
  quality: "high",
  customBitrateKbps: 20000,
  bufferSeconds: 30,
  micEnabled: false,
  micDeviceId: null,
  hotkey: "Alt+Backquote",
  outputDir: "/home/u/Videos/Klipt",
  startWithOs: true,
  saveSound: true,
  saveNotification: true,
  serverUrl: null,
  cleanupEnabled: false,
  cleanupDays: 30,
  friendNotifications: true,
  voiceApps: ["discord", "vesktop"],
};

const fields = (patch: Partial<Settings>) => validateSettings({ ...defaults, ...patch }).map((e) => e.field);

describe("estimateRamMb", () => {
  it("is bitrate × seconds / 8, in MB", () => {
    expect(estimateRamMb(20000, 30)).toBe(75);
    expect(estimateRamMb(8000, 10)).toBe(10);
    expect(estimateRamMb(80000, 600)).toBe(6000);
  });

  it("rounds to whole megabytes", () => {
    expect(estimateRamMb(14000, 45)).toBe(79); // 78.75
    expect(estimateRamMb(2000, 11)).toBe(3); // 2.75
  });
});

describe("bitrateKbps", () => {
  it("maps presets and uses the custom value for Custom", () => {
    expect(bitrateKbps({ ...defaults, quality: "low" })).toBe(8000);
    expect(bitrateKbps({ ...defaults, quality: "medium" })).toBe(14000);
    expect(bitrateKbps({ ...defaults, quality: "high", customBitrateKbps: 5000 })).toBe(20000);
    expect(bitrateKbps({ ...defaults, quality: "custom", customBitrateKbps: 35000 })).toBe(35000);
  });
});

describe("validateSettings mirrors Settings::validate", () => {
  it("accepts the defaults", () => {
    expect(validateSettings(defaults)).toEqual([]);
  });

  it("checks the buffer length range 10–600", () => {
    expect(fields({ bufferSeconds: 9 })).toEqual(["bufferSeconds"]);
    expect(fields({ bufferSeconds: 601 })).toEqual(["bufferSeconds"]);
    expect(fields({ bufferSeconds: 10 })).toEqual([]);
    expect(fields({ bufferSeconds: 600 })).toEqual([]);
    expect(fields({ bufferSeconds: 30.5 })).toEqual(["bufferSeconds"]);
    expect(fields({ bufferSeconds: Number.NaN })).toEqual(["bufferSeconds"]);
  });

  it("allows only 30 or 60 fps", () => {
    expect(fields({ fps: 45 })).toEqual(["fps"]);
    expect(fields({ fps: 30 })).toEqual([]);
  });

  it("checks the custom bitrate range 2000–80000 even when a preset is chosen", () => {
    expect(fields({ customBitrateKbps: 1999, quality: "custom" })).toEqual(["customBitrateKbps"]);
    expect(fields({ customBitrateKbps: 1999, quality: "high" })).toEqual([]);
    expect(fields({ customBitrateKbps: 80001, quality: "custom" })).toEqual(["customBitrateKbps"]);
    expect(fields({ customBitrateKbps: 2000 })).toEqual([]);
    expect(fields({ customBitrateKbps: 80000 })).toEqual([]);
  });

  it("rejects recording to disk", () => {
    expect(fields({ bufferLocation: "disk" })).toEqual(["bufferLocation"]);
  });

  it("needs a hotkey and a clips folder", () => {
    expect(fields({ hotkey: "  " })).toEqual(["hotkey"]);
    expect(fields({ outputDir: "" })).toEqual(["outputDir"]);
  });

  it("uses the same messages and order as the core", () => {
    const errors = validateSettings({ ...defaults, bufferLocation: "disk", fps: 45, quality: "custom", customBitrateKbps: 1, bufferSeconds: 5, hotkey: "", outputDir: "" });
    expect(errors).toEqual([
      { field: "bufferLocation", message: "Recording to disk isn't supported yet." },
      { field: "fps", message: "Frame rate must be 30 or 60." },
      { field: "customBitrateKbps", message: "Bitrate must be between 2,000 and 80,000 kbps." },
      { field: "bufferSeconds", message: "Clip length must be between 10 seconds and 10 minutes." },
      { field: "hotkey", message: "Choose a shortcut for saving clips." },
      { field: "outputDir", message: "Choose a folder for your clips." },
    ]);
  });
});

describe("sameSettings", () => {
  it("compares every field", () => {
    expect(sameSettings(defaults, { ...defaults })).toBe(true);
    expect(sameSettings(defaults, { ...defaults, fps: 30 })).toBe(false);
    expect(sameSettings(defaults, { ...defaults, displayId: "DP-1" })).toBe(false);
  });
});

describe("cleanup days", () => {
  it("must be a whole number from 1 to 365", () => {
    expect(fields({ cleanupDays: 1 })).toEqual([]);
    expect(fields({ cleanupDays: 365 })).toEqual([]);
    expect(fields({ cleanupDays: 0 })).toEqual(["cleanupDays"]);
    expect(fields({ cleanupDays: 366 })).toEqual(["cleanupDays"]);
    expect(fields({ cleanupDays: 2.5 })).toEqual(["cleanupDays"]);
  });
});

describe("voice chat apps", () => {
  it("parses a comma list and checks the names", () => {
    expect(parseAppList(" discord, Vesktop ,, ")).toEqual(["discord", "Vesktop"]);
    expect(fields({ voiceApps: [] })).toEqual([]);
    expect(fields({ voiceApps: ["a|b"] })).toEqual(["voiceApps"]);
    expect(fields({ voiceApps: Array(11).fill("x") })).toEqual(["voiceApps"]);
  });

  it("compares lists by content for unsaved changes", () => {
    expect(sameSettings(defaults, { ...defaults, voiceApps: ["discord", "vesktop"] })).toBe(true);
    expect(sameSettings(defaults, { ...defaults, voiceApps: ["discord"] })).toBe(false);
  });
});
