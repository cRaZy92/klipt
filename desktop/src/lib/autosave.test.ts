import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Autosave, type SaveStatus } from "./autosave";

/** A save the test resolves or rejects by hand. */
function deferred() {
  let resolve!: () => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<void>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function setup() {
  let dirty = false;
  let blocked = false;
  const saves: ReturnType<typeof deferred>[] = [];
  const statuses: SaveStatus[] = [];
  const errors: unknown[] = [];
  const autosave = new Autosave({
    save: () => {
      const d = deferred();
      saves.push(d);
      return d.promise;
    },
    dirty: () => dirty,
    blocked: () => blocked,
    onstatus: (s, e) => {
      statuses.push(s);
      if (e !== undefined) errors.push(e);
    },
    delayMs: 600,
  });
  return {
    autosave,
    saves,
    statuses,
    errors,
    setDirty: (d: boolean) => (dirty = d),
    setBlocked: (b: boolean) => (blocked = b),
  };
}

/** Lets pending promise callbacks run. */
const settle = () => vi.advanceTimersByTimeAsync(0);

describe("Autosave", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("saves once, 600 ms after the last change", async () => {
    const { autosave, saves, setDirty } = setup();
    setDirty(true);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(400);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(599);
    expect(saves).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1);
    expect(saves).toHaveLength(1);
  });

  it("reports saving while a change waits, and saved once it is written", async () => {
    const { autosave, saves, statuses, setDirty } = setup();
    setDirty(true);
    autosave.changed();
    expect(statuses.at(-1)).toBe("saving");
    await vi.advanceTimersByTimeAsync(600);
    setDirty(false);
    saves[0].resolve();
    await settle();
    expect(statuses.at(-1)).toBe("saved");
    expect(autosave.pending).toBe(false);
  });

  it("doesn't save when nothing differs from the saved edit", async () => {
    const { autosave, saves, statuses } = setup();
    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    expect(saves).toHaveLength(0);
    expect(statuses.at(-1)).toBe("saved");
  });

  it("a failed save is reported, stays pending, and is retried on the next change", async () => {
    const { autosave, saves, statuses, errors, setDirty } = setup();
    setDirty(true);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    saves[0].reject("busy");
    await settle();
    expect(statuses.at(-1)).toBe("failed");
    expect(errors).toEqual(["busy"]);
    expect(autosave.pending).toBe(true);

    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    expect(saves).toHaveLength(2);
    setDirty(false);
    saves[1].resolve();
    await settle();
    expect(statuses.at(-1)).toBe("saved");
    expect(autosave.pending).toBe(false);
  });

  it("flush saves a waiting change at once and says whether it worked", async () => {
    const { autosave, saves, setDirty } = setup();
    setDirty(true);
    autosave.changed();
    const flushed = autosave.flush();
    expect(saves).toHaveLength(1);
    setDirty(false);
    saves[0].resolve();
    expect(await flushed).toBe(true);
    // The timer was dropped: no second save.
    await vi.advanceTimersByTimeAsync(1000);
    expect(saves).toHaveLength(1);
  });

  it("flush retries a failed save and reports a second failure", async () => {
    const { autosave, saves, setDirty } = setup();
    setDirty(true);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    saves[0].reject("busy");
    await settle();
    const flushed = autosave.flush();
    expect(saves).toHaveLength(2);
    saves[1].reject("still busy");
    expect(await flushed).toBe(false);
    expect(autosave.pending).toBe(true);
  });

  it("a change made while saving is saved after that save, not alongside it", async () => {
    const { autosave, saves, statuses, setDirty } = setup();
    setDirty(true);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    expect(saves).toHaveLength(1);
    expect(autosave.pending).toBe(true);
    saves[0].resolve();
    await settle();
    expect(saves).toHaveLength(2);
    expect(statuses.at(-1)).toBe("saving");
    setDirty(false);
    saves[1].resolve();
    await settle();
    expect(statuses.at(-1)).toBe("saved");
  });

  it("flush during a save waits for it, then saves what changed meanwhile", async () => {
    const { autosave, saves, setDirty } = setup();
    setDirty(true);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    const flushed = autosave.flush();
    saves[0].resolve();
    await settle();
    expect(saves).toHaveLength(2);
    setDirty(false);
    saves[1].resolve();
    expect(await flushed).toBe(true);
  });

  it("stop drops a waiting save", async () => {
    const { autosave, saves, setDirty } = setup();
    setDirty(true);
    autosave.changed();
    autosave.stop();
    await vi.advanceTimersByTimeAsync(1000);
    expect(saves).toHaveLength(0);
  });

  it("while blocked (e.g. uploading) a change waits instead of saving, and stays pending", async () => {
    const { autosave, saves, statuses, errors, setDirty, setBlocked } = setup();
    setDirty(true);
    setBlocked(true);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    expect(saves).toHaveLength(0);
    expect(statuses.at(-1)).toBe("waiting");
    expect(errors).toEqual([]);
    expect(autosave.pending).toBe(true);

    // Unblocked: flushing saves the waiting change.
    setBlocked(false);
    const flushed = autosave.flush();
    expect(saves).toHaveLength(1);
    setDirty(false);
    saves[0].resolve();
    expect(await flushed).toBe(true);
    expect(statuses.at(-1)).toBe("saved");
    expect(autosave.pending).toBe(false);
  });

  it("a waiting change that was undone back to the saved edit is no longer pending", async () => {
    const { autosave, statuses, setDirty, setBlocked } = setup();
    setDirty(true);
    setBlocked(true);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    setDirty(false);
    autosave.changed();
    await vi.advanceTimersByTimeAsync(600);
    expect(statuses.at(-1)).toBe("saved");
    expect(autosave.pending).toBe(false);
  });

  it("flush while blocked reports that nothing was saved", async () => {
    const { autosave, saves, setDirty, setBlocked } = setup();
    setDirty(true);
    setBlocked(true);
    expect(await autosave.flush()).toBe(false);
    expect(saves).toHaveLength(0);
  });
});
