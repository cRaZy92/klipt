import { beforeEach, describe, expect, it, vi } from "vitest";
import { cached, forgetCache, refresh } from "./cache";

beforeEach(forgetCache);

describe("cache", () => {
  it("cached returns undefined before any refresh", () => {
    expect(cached("a")).toBeUndefined();
  });

  it("refresh stores and returns the loaded value", async () => {
    expect(await refresh("a", async () => 1)).toBe(1);
    expect(cached("a")).toBe(1);
  });

  it("concurrent refreshes share one load", async () => {
    let resolve!: (v: number) => void;
    const load = vi.fn(() => new Promise<number>((r) => (resolve = r)));
    const first = refresh("a", load);
    const second = refresh("a", load);
    resolve(7);
    expect(await first).toBe(7);
    expect(await second).toBe(7);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("a failed refresh keeps the old value", async () => {
    await refresh("a", async () => 1);
    await expect(refresh("a", () => Promise.reject(new Error("x")))).rejects.toThrow("x");
    expect(cached("a")).toBe(1);
  });

  it("forgetCache clears values and ignores loads still running", async () => {
    await refresh("b", async () => 2);
    let resolve!: (v: number) => void;
    const running = refresh("a", () => new Promise<number>((r) => (resolve = r)));
    forgetCache();
    resolve(1);
    await running;
    expect(cached("a")).toBeUndefined();
    expect(cached("b")).toBeUndefined();
  });
});
