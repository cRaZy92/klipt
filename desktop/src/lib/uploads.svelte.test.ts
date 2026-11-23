import { describe, expect, it, vi } from "vitest";

vi.mock("./api", async (orig) => ({
  ...(await orig<typeof import("./api")>()),
  api: {
    pendingUploads: async () => [
      { clipId: "c1", title: "Ace", visibility: "link", tagged: [], replace: false, queuedAtMs: 1 },
    ],
  },
  on: () => () => {},
}));

import { loadPendingUploads, uploads } from "./uploads.svelte";

describe("loadPendingUploads", () => {
  it("shows saved uploads as queued cards", async () => {
    await loadPendingUploads();
    expect(uploads.c1.status).toBe("queued");
    expect(uploads.c1.title).toBe("Ace");
    expect(uploads.c1.visibility).toBe("link");
  });
});
