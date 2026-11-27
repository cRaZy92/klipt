import { describe, expect, it } from "vitest";
import { nextVisibilityToast, toVisibility, VISIBILITY } from "./visibility";

describe("visibility", () => {
  it("reads the old feed as friends, unknown or missing as null", () => {
    expect(toVisibility("feed")).toBe("friends");
    expect(toVisibility("friends")).toBe("friends");
    expect(toVisibility("public")).toBe("public");
    expect(toVisibility("link")).toBe("link");
    expect(toVisibility(null)).toBeNull();
    expect(toVisibility("other")).toBeNull();
  });

  it("lists the three levels in order with labels", () => {
    expect(VISIBILITY.map((v) => v.value)).toEqual(["link", "friends", "public"]);
    expect(VISIBILITY.map((v) => v.label)).toEqual(["Link only", "Friends", "Public"]);
  });

  it("says what changed", () => {
    expect(nextVisibilityToast("link")).toBe("Now link-only.");
    expect(nextVisibilityToast("friends")).toBe("Now friends only.");
    expect(nextVisibilityToast("public")).toBe("Now public.");
  });
});
