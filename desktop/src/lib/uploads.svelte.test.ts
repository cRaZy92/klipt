import { describe, expect, it, vi } from "vitest";

const row = (clipId: string, status: string) => ({
  clipId,
  title: `T ${clipId}`,
  visibility: "link",
  tagged: [],
  replace: false,
  queuedAtMs: 1,
  status,
});

vi.mock("./api", async (orig) => ({
  ...(await orig<typeof import("./api")>()),
  api: {
    pendingUploads: async () => [row("q", "queued"), row("r", "running"), row("l", "needsLogin"), row("w", "waiting")],
  },
  on: () => () => {},
}));

import { isBusy, loadPendingUploads, uploads } from "./uploads.svelte";

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
