import { describe, expect, it } from "vitest";
import { feedNeedsRefresh } from "./feedRefresh";

describe("feedNeedsRefresh", () => {
  it("refreshes a feed never loaded", () => {
    expect(feedNeedsRefresh({ loadedAt: 0, pages: 0 }, 1_000)).toBe(true);
  });

  it("keeps a feed loaded less than 30 s ago", () => {
    expect(feedNeedsRefresh({ loadedAt: 100_000, pages: 1 }, 129_000)).toBe(false);
  });

  it("refreshes a first page older than 30 s", () => {
    expect(feedNeedsRefresh({ loadedAt: 100_000, pages: 1 }, 131_000)).toBe(true);
  });

  it("keeps older pages the user loaded, however old", () => {
    expect(feedNeedsRefresh({ loadedAt: 100_000, pages: 3 }, 1_000_000)).toBe(false);
  });
});
