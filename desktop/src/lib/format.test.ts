import { describe, expect, it } from "vitest";
import {
  bufferLabel,
  dayGroup,
  defaultTitle,
  exportFileName,
  formatDuration,
  formatSize,
  groupByDay,
  hotkeyKeys,
  hotkeyLabel,
  hueOf,
  relativeDate,
  uploadTitle,
} from "./format";

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

describe("dayGroup", () => {
  const now = new Date(2026, 10, 11, 15, 0, 0); // 11 Nov 2026, 15:00 local

  it("names today and yesterday by the calendar day, not 24 hours", () => {
    expect(dayGroup(new Date(2026, 10, 11, 0, 5), now)).toBe("Today");
    expect(dayGroup(new Date(2026, 10, 10, 23, 55), now)).toBe("Yesterday");
    expect(dayGroup(new Date(2026, 10, 10, 0, 1), now)).toBe("Yesterday");
  });

  it("treats a clock that's a little ahead as today", () => {
    expect(dayGroup(new Date(2026, 10, 11, 15, 1), now)).toBe("Today");
  });

  it("groups the rest of the week, then the rest of the month", () => {
    expect(dayGroup(new Date(2026, 10, 9), now)).toBe("Last 7 days");
    expect(dayGroup(new Date(2026, 10, 5), now)).toBe("Last 7 days");
    expect(dayGroup(new Date(2026, 10, 4), now)).toBe("Earlier this month");
    expect(dayGroup(new Date(2026, 10, 1), now)).toBe("Earlier this month");
  });

  it("names older months, with the year only when it isn't this year", () => {
    expect(dayGroup(new Date(2026, 9, 31), now)).toBe("October");
    expect(dayGroup(new Date(2025, 10, 20), now)).toBe("November 2025");
  });

  it("puts the last days of the previous month in the week group", () => {
    const early = new Date(2026, 10, 2, 12, 0);
    expect(dayGroup(new Date(2026, 9, 30), early)).toBe("Last 7 days");
  });
});

describe("groupByDay", () => {
  const now = new Date(2026, 10, 11, 15, 0, 0);
  const at = (d: number, h = 12) => new Date(2026, 10, d, h).getTime();

  it("keeps the order and starts a group where the heading changes", () => {
    const groups = groupByDay([at(11, 14), at(11, 9), at(10), at(8), at(7), at(2)], (t) => t, now);
    expect(groups.map((g) => [g.label, g.items.length])).toEqual([
      ["Today", 2],
      ["Yesterday", 1],
      ["Last 7 days", 2],
      ["Earlier this month", 1],
    ]);
  });

  it("works oldest first too, with unique keys", () => {
    const groups = groupByDay([at(2), at(10), at(11)], (t) => t, now);
    expect(groups.map((g) => g.label)).toEqual(["Earlier this month", "Yesterday", "Today"]);
    expect(new Set(groups.map((g) => g.key)).size).toBe(3);
  });

  it("keys groups by label, so they keep their key when an earlier group goes away", () => {
    const before = groupByDay([at(11), at(10), at(8)], (t) => t, now);
    const after = groupByDay([at(11), at(8)], (t) => t, now);
    expect(before.at(-1)!.key).toBe(after.at(-1)!.key);
  });

  it("still gives unique keys when a heading comes back in an unsorted list", () => {
    const groups = groupByDay([at(11), at(10), at(11)], (t) => t, now);
    expect(groups.map((g) => g.label)).toEqual(["Today", "Yesterday", "Today"]);
    expect(new Set(groups.map((g) => g.key)).size).toBe(3);
  });

  it("gives an empty list no groups", () => {
    expect(groupByDay([], (t: number) => t, now)).toEqual([]);
  });
});

describe("hueOf", () => {
  it("is stable, in range and ignores case and outer spaces", () => {
    const hue = hueOf("Deep Rock Galactic");
    expect(hue).toBeGreaterThanOrEqual(0);
    expect(hue).toBeLessThan(360);
    expect(hueOf(" deep rock galactic ")).toBe(hue);
  });

  it("spreads names over several colours", () => {
    const names = ["Counter-Strike 2", "Deep Rock Galactic", "Helldivers 2", "Rocket League", "Elden Ring", "Apex Legends"];
    expect(new Set(names.map(hueOf)).size).toBeGreaterThanOrEqual(4);
  });
});

describe("hotkeyKeys", () => {
  it("splits a shortcut into keys", () => {
    expect(hotkeyKeys("Alt+`")).toEqual(["Alt", "`"]);
    expect(hotkeyKeys("Ctrl + Shift + K")).toEqual(["Ctrl", "Shift", "K"]);
    expect(hotkeyKeys("Alt+;")).toEqual(["Alt", ";"]);
  });

  it("keeps a plus key", () => {
    expect(hotkeyKeys("Ctrl++")).toEqual(["Ctrl", "+"]);
    expect(hotkeyKeys("Ctrl + +")).toEqual(["Ctrl", "+"]);
  });

  it("gives nothing for an empty shortcut", () => {
    expect(hotkeyKeys("")).toEqual([]);
  });
});

describe("formatSize", () => {
  it("uses MB below a gigabyte and GB above, one decimal below 10", () => {
    expect(formatSize(0)).toBe("0 MB");
    expect(formatSize(10)).toBe("0.1 MB");
    expect(formatSize(340 * 1024 ** 2)).toBe("340 MB");
    expect(formatSize(1.5 * 1024 ** 3)).toBe("1.5 GB");
    expect(formatSize(1024 ** 3)).toBe("1 GB");
    expect(formatSize(12 * 1024 ** 3)).toBe("12 GB");
  });
});
