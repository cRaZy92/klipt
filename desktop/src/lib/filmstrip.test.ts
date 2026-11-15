import { describe, expect, it } from "vitest";
import { frameIndex, frameUrl, previewIndex } from "./filmstrip";

const asPath = (p: string) => p;

describe("frameIndex", () => {
  it("rounds to the nearest 100 ms picture", () => {
    expect(frameIndex(0)).toBe(0);
    expect(frameIndex(149)).toBe(1);
    expect(frameIndex(150)).toBe(2);
  });

  it("never goes below the first picture", () => {
    expect(frameIndex(-5)).toBe(0);
  });
});

describe("previewIndex", () => {
  it("is the picture at ms while it is ready", () => {
    expect(previewIndex(1000, 11)).toBe(10);
  });

  it("is null while that picture isn't made yet", () => {
    expect(previewIndex(1000, 10)).toBeNull();
    expect(previewIndex(0, 0)).toBeNull();
  });
});

describe("frameUrl", () => {
  it("names the picture with its index padded to 6 digits", () => {
    expect(frameUrl("/c/edit/a-strip", 7, asPath)).toMatch(/000007\.jpg$/);
    expect(frameUrl("/c/edit/a-strip", 7, asPath)).toBe("/c/edit/a-strip/000007.jpg");
  });

  it("joins with the separator a Windows dir uses", () => {
    expect(frameUrl("C:\\Klipt\\edit\\a-strip", 123, asPath)).toBe("C:\\Klipt\\edit\\a-strip\\000123.jpg");
  });

  it("joins with / when the dir mixes separators", () => {
    expect(frameUrl("C:\\Klipt/edit/a-strip", 1, asPath)).toBe("C:\\Klipt/edit/a-strip/000001.jpg");
  });

  it("turns the path into a URL", () => {
    expect(frameUrl("/strip", 0, (p) => `asset://${p}`)).toBe("asset:///strip/000000.jpg");
  });
});
