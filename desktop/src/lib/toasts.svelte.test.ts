import { afterEach, describe, expect, it, vi } from "vitest";
import { toast, toasts } from "./toasts.svelte";

describe("toast", () => {
  afterEach(() => vi.useRealTimers());

  it("with an action keeps it and stays longer", () => {
    vi.useFakeTimers();
    const run = vi.fn();
    toast("Clip uploaded", "success", { label: "Copy link", run });
    vi.advanceTimersByTime(5000);
    const shown = toasts.find((t) => t.message === "Clip uploaded");
    expect(shown?.action?.label).toBe("Copy link");
    shown?.action?.run();
    expect(run).toHaveBeenCalled();
    vi.advanceTimersByTime(4000);
    expect(toasts.find((t) => t.message === "Clip uploaded")).toBeUndefined();
  });
});
