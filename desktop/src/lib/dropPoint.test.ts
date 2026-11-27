import { describe, expect, it } from "vitest";
import { dropPoint, isNetworkAddress } from "./dropPoint";

describe("dropPoint", () => {
  it("scales physical pixels down on Windows only", () => {
    expect(dropPoint({ x: 300, y: 150 }, true, 1.5)).toEqual({ x: 200, y: 100 });
    expect(dropPoint({ x: 300, y: 150 }, false, 1.5)).toEqual({ x: 300, y: 150 });
    expect(dropPoint({ x: 30, y: 15 }, true, 0)).toEqual({ x: 30, y: 15 });
  });
});

describe("isNetworkAddress", () => {
  it("spots share addresses, not paths", () => {
    expect(isNetworkAddress("smb://nas/share/clip.mp4")).toBe(true);
    expect(isNetworkAddress("sftp://me@host/clip.mp4")).toBe(true);
    expect(isNetworkAddress("/home/me/clip.mp4")).toBe(false);
    expect(isNetworkAddress("file:///home/me/clip.mp4")).toBe(false);
    expect(isNetworkAddress("C:\\Videos\\clip.mp4")).toBe(false);
    expect(isNetworkAddress("\\\\nas\\share\\clip.mp4")).toBe(false);
  });
});
