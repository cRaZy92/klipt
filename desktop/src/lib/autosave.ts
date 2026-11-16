/**
 * What the header says about saving: a change is waiting or being written, all saved, the last
 * save failed, or a change waits until saving is possible again (e.g. while the clip uploads).
 */
export type SaveStatus = "saving" | "saved" | "failed" | "waiting";

export interface AutosaveOptions {
  /** Writes the edit as it is now. */
  save: () => Promise<void>;
  /** Whether the edit differs from the saved one (and can be saved). */
  dirty: () => boolean;
  /** Saving isn't possible now (a change then waits, without an error, until `flush`). */
  blocked?: () => boolean;
  /** The status changed; `error` comes with "failed". */
  onstatus: (status: SaveStatus, error?: unknown) => void;
  /** How long after the last change the save starts. */
  delayMs?: number;
}

/**
 * Saves an edit a moment after the last change, one save at a time: a change made while saving
 * is saved after that save, and so is an edit that still differs from what that save wrote. A failed save stays pending until the next change or `flush`.
 */
export class Autosave {
  private timer: ReturnType<typeof setTimeout> | undefined;
  private running = false;
  /** The save loop in progress (settled once `running` is false). */
  private done: Promise<void> = Promise.resolve();
  /** Asked to save while a save was running: save again after it. */
  private again = false;
  private failed = false;
  /** A change waits for saving to be possible again. */
  private waiting = false;

  constructor(private readonly options: AutosaveOptions) {}

  /** A change is waiting to be saved or being saved, or the last save failed. */
  get pending(): boolean {
    return this.timer !== undefined || this.running || this.again || this.failed || this.waiting;
  }

  /** The edit changed: saves it once changes rest for the delay. */
  changed(): void {
    this.cancelTimer();
    this.timer = setTimeout(() => {
      this.timer = undefined;
      void this.run();
    }, this.options.delayMs ?? 600);
    this.options.onstatus("saving");
  }

  /** Saves now (after a save in progress); resolves whether everything is saved. */
  async flush(): Promise<boolean> {
    this.cancelTimer();
    await this.run();
    return !this.failed && !this.waiting;
  }

  /** Drops a waiting save (the editor is closing and saves by itself). */
  stop(): void {
    this.cancelTimer();
    this.again = false;
  }

  private cancelTimer() {
    clearTimeout(this.timer);
    this.timer = undefined;
  }

  private run(): Promise<void> {
    if (this.running) {
      this.again = true;
      return this.done;
    }
    this.running = true;
    this.done = this.loop();
    return this.done;
  }

  private async loop(): Promise<void> {
    try {
      do {
        this.again = false;
        if (!this.options.dirty()) {
          this.failed = false;
          this.waiting = false;
          continue;
        }
        if (this.options.blocked?.()) {
          this.waiting = true;
          this.again = false;
          this.options.onstatus("waiting");
          return;
        }
        this.waiting = false;
        this.options.onstatus("saving");
        try {
          await this.options.save();
          this.failed = false;
          // The edit changed during the save without a change waiting for its timer (e.g. it went
          // back to the edit stored before this save): it differs from what was just written.
          if (this.timer === undefined && this.options.dirty()) this.again = true;
        } catch (e) {
          this.failed = true;
          this.again = false;
          this.options.onstatus("failed", e);
          return;
        }
      } while (this.again);
      // A change made meanwhile is still waiting for its timer: still "saving".
      if (this.timer === undefined) this.options.onstatus("saved");
    } finally {
      this.running = false;
    }
  }
}
