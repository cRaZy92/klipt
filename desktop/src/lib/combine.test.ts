import { describe, expect, it } from "vitest";
import { combinable, combinedLength, combinedTitle, moveItem } from "./combine";

const clip = (id: string, createdAt: number, extra: Partial<Parameters<typeof combinable>[0][number]> = {}) => ({
  id,
  kind: "clip" as const,
  fileMissing: false,
  createdAt,
  gameName: "CS2" as string | null,
  ...extra,
});

describe("combinable", () => {
  it("keeps clips with their file, oldest first", () => {
    const got = combinable([
      clip("b", 20),
      clip("shot", 5, { kind: "screenshot" }),
      clip("a", 10),
      clip("gone", 1, { fileMissing: true }),
    ]);
    expect(got.map((c) => c.id)).toEqual(["a", "b"]);
  });
});

describe("moveItem", () => {
  it("swaps with the neighbour and stays put at the ends", () => {
    expect(moveItem(["a", "b", "c"], 1, -1)).toEqual(["b", "a", "c"]);
    expect(moveItem(["a", "b", "c"], 1, 1)).toEqual(["a", "c", "b"]);
    expect(moveItem(["a", "b", "c"], 0, -1)).toEqual(["a", "b", "c"]);
    expect(moveItem(["a", "b", "c"], 2, 1)).toEqual(["a", "b", "c"]);
  });
});

describe("combinedTitle", () => {
  it("names one game, or none", () => {
    expect(combinedTitle([{ gameName: "CS2" }, { gameName: "CS2" }])).toBe("CS2 highlights");
    expect(combinedTitle([{ gameName: "CS2" }, { gameName: "Dota 2" }])).toBe("Highlights");
    expect(combinedTitle([{ gameName: null }, { gameName: null }])).toBe("Highlights");
  });
});

describe("combinedLength", () => {
  it("adds up the edited lengths", () => {
    expect(
      combinedLength([
        { durationMs: 10_000, trimStartMs: 2_000, trimEndMs: 8_000, removedRanges: [{ startMs: 3_000, endMs: 4_000 }] },
        { durationMs: 5_000, trimStartMs: null, trimEndMs: null, removedRanges: [] },
      ]),
    ).toBe(10_000);
  });
});
