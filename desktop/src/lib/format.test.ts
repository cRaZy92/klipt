import { describe, expect, it } from "vitest";
import { bufferLabel, defaultTitle, exportFileName, formatDuration, hotkeyLabel, relativeDate, uploadTitle } from "./format";

describe("formatDuration", () => {
  it("shows minutes and zero-padded seconds", () => {
    expect(formatDuration(65_000)).toBe("1:05");
    expect(formatDuration(30_000)).toBe("0:30");
    expect(formatDuration(0)).toBe("0:00");
  });

  it("rounds to the nearest second", () => {
    expect(formatDuration(29_980)).toBe("0:30");
    expect(formatDuration(1_400)).toBe("0:01");
  });

  it("adds hours from one hour up", () => {
    expect(formatDuration(3_665_000)).toBe("1:01:05");
    expect(formatDuration(3_600_000)).toBe("1:00:00");
    expect(formatDuration(3_599_000)).toBe("59:59");
  });

  it("treats bad input as zero", () => {
    expect(formatDuration(-5)).toBe("0:00");
    expect(formatDuration(Number.NaN)).toBe("0:00");
  });
});

describe("relativeDate", () => {
  const now = new Date(2026, 10, 9, 15, 0, 0); // 9 Nov 2026, 15:00 local

  it("says just now under a minute", () => {
    expect(relativeDate(new Date(2026, 10, 9, 14, 59, 30), now)).toBe("just now");
    expect(relativeDate(now, now)).toBe("just now");
  });

  it("counts minutes under an hour", () => {
    expect(relativeDate(new Date(2026, 10, 9, 14, 55), now)).toBe("5 min ago");
    expect(relativeDate(new Date(2026, 10, 9, 14, 1), now)).toBe("59 min ago");
  });

  it("counts hours under a day", () => {
    expect(relativeDate(new Date(2026, 10, 9, 12, 0), now)).toBe("3 h ago");
    expect(relativeDate(new Date(2026, 10, 8, 16, 0), now)).toBe("23 h ago");
  });

  it("says yesterday for the previous calendar day beyond 24 hours", () => {
    expect(relativeDate(new Date(2026, 10, 8, 9, 0), now)).toBe("yesterday");
  });

  it("shows the date for older clips, with the year only when it differs", () => {
    expect(relativeDate(new Date(2026, 10, 7, 9, 0), now)).toBe("7 Nov");
    expect(relativeDate(new Date(2026, 8, 1, 9, 0), now)).toBe("1 Sep");
    expect(relativeDate(new Date(2025, 10, 9, 9, 0), now)).toBe("9 Nov 2025");
  });

  it("accepts unix milliseconds", () => {
    expect(relativeDate(now.getTime() - 5 * 60_000, now.getTime())).toBe("5 min ago");
  });

  it("treats a date slightly in the future as just now", () => {
    expect(relativeDate(new Date(2026, 10, 9, 15, 2), now)).toBe("just now");
  });
});

describe("defaultTitle", () => {
  const date = new Date(2026, 10, 9, 21, 30);

  it("is the game and the date with an em dash", () => {
    expect(defaultTitle("Active Matter", date)).toBe("Active Matter — 9 Nov 2026");
  });

  it("uses Clip for an unknown game", () => {
    expect(defaultTitle(null, date)).toBe("Clip — 9 Nov 2026");
    expect(defaultTitle("  ", date)).toBe("Clip — 9 Nov 2026");
  });

  it("uses three-letter months", () => {
    expect(defaultTitle("CS2", new Date(2026, 8, 15))).toBe("CS2 — 15 Sep 2026");
  });
});

describe("uploadTitle", () => {
  const createdAt = new Date(2026, 10, 9, 21, 30).getTime();

  it("suggests the default title for a clip the user hasn't renamed", () => {
    const clip = { title: "Active Matter", gameName: "Active Matter", path: "/v/Replay_2026.mp4", createdAt };
    expect(uploadTitle(clip)).toBe("Active Matter — 9 Nov 2026");
    const unknown = { title: "Replay_2026", gameName: null, path: "C:\\Videos\\Klipt\\Replay_2026.mp4", createdAt };
    expect(uploadTitle(unknown)).toBe("Clip — 9 Nov 2026");
  });

  it("keeps a title the user chose", () => {
    const clip = { title: "Triple kill", gameName: "Active Matter", path: "/v/Replay_2026.mp4", createdAt };
    expect(uploadTitle(clip)).toBe("Triple kill");
  });
});

describe("hotkeyLabel", () => {
  it("spells keys the way they look on the keyboard", () => {
    expect(hotkeyLabel("Alt+Backquote")).toBe("Alt + `");
    expect(hotkeyLabel("Ctrl+Shift+KeyS")).toBe("Ctrl + Shift + S");
    expect(hotkeyLabel("Alt+Digit1")).toBe("Alt + 1");
    expect(hotkeyLabel("Alt+F9")).toBe("Alt + F9");
  });
});

describe("bufferLabel", () => {
  it("uses minutes when they are whole", () => {
    expect(bufferLabel(30)).toBe("30 seconds");
    expect(bufferLabel(60)).toBe("1 minute");
    expect(bufferLabel(120)).toBe("2 minutes");
    expect(bufferLabel(90)).toBe("90 seconds");
  });
});

describe("exportFileName", () => {
  it("names the file after the title", () => {
    expect(exportFileName("Active Matter — 9 Nov 2026")).toBe("Active Matter — 9 Nov 2026.mp4");
  });

  it("drops characters Windows and Linux file names can't have", () => {
    expect(exportFileName('a/b\\c: "d"?*<e>|')).toBe("a b c d e.mp4");
    expect(exportFileName("ends with dots...")).toBe("ends with dots.mp4");
  });

  it("falls back to Clip", () => {
    expect(exportFileName("  ///  ")).toBe("Clip.mp4");
  });
});
