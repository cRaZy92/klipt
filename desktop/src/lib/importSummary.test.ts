import { describe, expect, it } from "vitest";
import { elapsedText, etaText, sizeText, summaryText, videoLengthText } from "./importSummary";

const base = { imported: 0, converted: 0, alreadyThere: 0, skipped: [], cancelled: false, elapsedSecs: 0 };

describe("summaryText", () => {
  it("says what was imported, converted, already there and skipped", () => {
    expect(summaryText({ ...base, imported: 38, converted: 5, alreadyThere: 2, skipped: [{ file: "a", reason: "b" }] })).toBe(
      "Imported 38 clips (5 converted to H.264). 2 were already in Klipt. 1 file skipped.",
    );
    expect(summaryText({ ...base, imported: 1, alreadyThere: 1 })).toBe("Imported 1 clip. 1 was already in Klipt.");
  });

  it("handles nothing new and a cancelled import", () => {
    expect(summaryText({ ...base, alreadyThere: 4 })).toBe("Nothing new to import. 4 were already in Klipt.");
    expect(summaryText({ ...base, cancelled: true })).toBe("Import stopped.");
    expect(summaryText({ ...base, imported: 3, cancelled: true })).toBe("Stopped after importing 3 clips.");
  });
});

describe("import formatting", () => {
  it("rounds time left to what a person needs", () => {
    expect(etaText(10)).toBe("less than a minute");
    expect(etaText(4 * 60 + 10)).toBe("about 4 min");
    expect(etaText(2 * 3600 + 10 * 60)).toBe("about 2 h 10 min");
    expect(etaText(3600)).toBe("about 1 h");
    expect(etaText(Number.NaN)).toBe("less than a minute");
  });

  it("formats elapsed time, video length and sizes", () => {
    expect(elapsedText(42)).toBe("0:42");
    expect(elapsedText(3729)).toBe("1:02:09");
    expect(videoLengthText(48_000)).toBe("48 s");
    expect(videoLengthText(125 * 60_000)).toBe("2 h 5 min");
    expect(sizeText(340 * 1024 ** 2)).toBe("340 MB");
    expect(sizeText(12.4 * 1024 ** 3)).toBe("12 GB");
    expect(sizeText(2.25 * 1024 ** 3)).toBe("2.3 GB");
  });
});
