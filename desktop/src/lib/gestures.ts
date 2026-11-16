/**
 * Pointer gestures (drags) from several sources at once: `active` while any is in progress, and
 * `onend` runs once when the last one ends (e.g. to record one undo step per drag).
 */
export class Gestures<Source extends string> {
  private readonly running = new Set<Source>();

  constructor(private readonly onend: () => void) {}

  get active(): boolean {
    return this.running.size > 0;
  }

  start(source: Source): void {
    this.running.add(source);
  }

  /** Ends `source`'s gesture; an end without a start is ignored. */
  end(source: Source): void {
    if (!this.running.delete(source)) return;
    if (this.running.size === 0) this.onend();
  }
}
