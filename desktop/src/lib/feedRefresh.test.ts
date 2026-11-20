import { describe, expect, it } from "vitest";
import { feedNeedsRefresh, isNewClip } from "./feedRefresh";

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

describe("isNewClip", () => {
  const clip = (createdAt: string, username = "bob") => ({ createdAt, owner: { username } });

  it("marks friends' clips shared after what you'd seen", () => {
    expect(isNewClip(clip("2026-11-20T12:00:00.000Z"), "2026-11-20T11:00:00.000Z", "adam")).toBe(true);
    expect(isNewClip(clip("2026-11-20T11:00:00.000Z"), "2026-11-20T11:00:00.000Z", "adam")).toBe(false);
  });

  it("never marks your own clips, or anything when it isn't known what you'd seen", () => {
    expect(isNewClip(clip("2026-11-20T12:00:00.000Z", "Adam"), "2026-11-20T11:00:00.000Z", "adam")).toBe(false);
    expect(isNewClip(clip("2026-11-20T12:00:00.000Z"), null, "adam")).toBe(false);
  });
});
