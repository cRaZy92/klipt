import { describe, expect, it, vi } from "vitest";

const row = (clipId: string, status: string) => ({
  clipId,
  title: `T ${clipId}`,
  visibility: "link",
  tagged: [],
  replace: false,
  queuedAtMs: 1,
  albums: clipId === "w" ? ["album0000001"] : [],
  status,
});

const { uploadClip } = vi.hoisted(() => ({ uploadClip: vi.fn(async (..._args: unknown[]) => {}) }));

vi.mock("./api", async (orig) => ({
  ...(await orig<typeof import("./api")>()),
  api: {
    pendingUploads: async () => [row("q", "queued"), row("r", "running"), row("l", "needsLogin"), row("w", "waiting")],
    uploadClip,
  },
  on: () => () => {},
}));

import { isBusy, loadPendingUploads, retryUpload, uploads } from "./uploads.svelte";

describe("loadPendingUploads", () => {
  it("shows saved uploads by what the core does with them", async () => {
    await loadPendingUploads();
    expect(uploads.q.status).toBe("queued");
    expect(uploads.q.title).toBe("T q");
    expect(uploads.q.visibility).toBe("link");
    expect(uploads.r.status).toBe("starting");
    expect(uploads.l.status).toBe("needsLogin");
    // Stopped (no connection): not stuck as busy, so "Try again" works.
    expect(uploads.w.status).toBe("failed");
    expect(isBusy(uploads.w)).toBe(false);
  });
});

describe("retryUpload", () => {
  it("keeps the albums a saved upload was going into", async () => {
    await loadPendingUploads();
    retryUpload("w");
    await vi.waitFor(() => expect(uploadClip).toHaveBeenCalled());
    expect(uploadClip.mock.calls[0]).toEqual(["w", "T w", "link", [], false, false, false, ["album0000001"]]);
  });
});
