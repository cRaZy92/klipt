import { describe, expect, it } from "vitest";
import type { Job } from "./api";
import { detail, ordered, percent, runningCount, timing } from "./jobs.svelte";

const job = (over: Partial<Job>): Job => ({
  id: 1,
  kind: "upload",
  title: "ace",
  clipId: null,
  state: "running",
  step: "Uploading",
  progress: 0.5,
  etaSecs: null,
  amount: null,
  message: null,
  startedAtMs: new Date(2026, 10, 28, 14, 2).getTime(),
  finishedAtMs: null,
  ...over,
});

describe("jobs", () => {
  it("shows a percentage, never 100% while still running", () => {
    expect(percent(job({ progress: 0.624 }))).toBe("62%");
    expect(percent(job({ progress: 0.999 }))).toBe("99%");
    expect(percent(job({ progress: 1, state: "done" }))).toBe("100%");
    expect(percent(job({ progress: 0.3, state: "failed" }))).toBe("30%");
  });

  it("adds sizes, speed and time left only where known", () => {
    const mb = 1024 * 1024;
    expect(detail(job({ amount: { unit: "bytes", done: 24 * mb, total: 38 * mb, rate: 3 * mb }, etaSecs: 4.2 }))).toBe(
      "24 MB of 38 MB · 3 MB/s · 5 s left",
    );
    expect(detail(job({ amount: { unit: "items", done: 29, total: 40 }, etaSecs: 130 }))).toBe("29 of 40 · 2 min left");
    expect(detail(job({}))).toBeNull();
    // Finished: no speed or time left.
    expect(detail(job({ state: "done", amount: { unit: "bytes", done: 38 * mb, total: 38 * mb, rate: 3 * mb }, etaSecs: 3 }))).toBe(
      "38 MB of 38 MB",
    );
  });

  it("lists running jobs first, oldest first, then the newest finished", () => {
    const list = [
      job({ id: 1, state: "done", finishedAtMs: 10 }),
      job({ id: 2, startedAtMs: 50 }),
      job({ id: 3, state: "failed", finishedAtMs: 20 }),
      job({ id: 4, startedAtMs: 5 }),
    ];
    expect(ordered(list).map((j) => j.id)).toEqual([4, 2, 3, 1]);
    expect(runningCount(list)).toBe(2);
  });

  it("says when it started and how long it took", () => {
    const start = new Date(2026, 10, 28, 14, 2).getTime();
    expect(timing(job({ startedAtMs: start }), start + 95_000)).toBe("Started 14:02 · 1:35 so far");
    expect(timing(job({ startedAtMs: start, state: "done", finishedAtMs: start + 192_000 }), 0)).toBe("14:02 → 14:05 · took 3:12");
  });
});
