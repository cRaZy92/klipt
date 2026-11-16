/**
 * Undo/redo history over plain JSON-able states. Records arriving within
 * `coalesceMs` of the previous record replace the newest step, so one drag
 * is one undo step. Undo/redo end any coalescing run.
 */
export class EditHistory<T> {
  private states: T[];
  private index = 0;
  private lastRecordAt: number | null = null;

  constructor(
    initial: T,
    private readonly coalesceMs = 400,
    private readonly now: () => number = () => performance.now(),
  ) {
    this.states = [clone(initial)];
  }

  get canUndo(): boolean {
    return this.index > 0;
  }

  get canRedo(): boolean {
    return this.index < this.states.length - 1;
  }

  record(state: T): void {
    if (JSON.stringify(state) === JSON.stringify(this.states[this.index])) return;
    const t = this.now();
    const coalesce =
      this.lastRecordAt !== null && t - this.lastRecordAt <= this.coalesceMs;
    this.lastRecordAt = t;
    this.states.length = this.index + 1;
    if (coalesce && this.index > 0) {
      this.states[this.index] = clone(state);
    } else {
      this.states.push(clone(state));
      this.index++;
    }
  }

  undo(): T | null {
    this.lastRecordAt = null;
    if (!this.canUndo) return null;
    this.index--;
    return clone(this.states[this.index]);
  }

  redo(): T | null {
    this.lastRecordAt = null;
    if (!this.canRedo) return null;
    this.index++;
    return clone(this.states[this.index]);
  }
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}
