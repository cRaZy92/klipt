import { describe, expect, it } from "vitest";
import {
  canRemove,
  clampToKept,
  deleteSplit,
  editedLength,
  keptLength,
  keptRanges,
  moveSplit,
  piecesFromRanges,
  playFrom,
  removedRanges,
  sameRanges,
  splitAt,
  toggle,
  wholeClip,
  type Piece,
} from "./cuts";

const D = 10_000;
const TRIM = { start: 1000, end: 9000 };
const p = (start: number, end: number, removed = false): Piece => ({ start, end, removed });
/** Split at 4000 and 6000 with the middle switched off. */
const middleOff = [p(0, 4000), p(4000, 6000, true), p(6000, D)];

describe("cuts", () => {
  it("splits at the playhead, refusing short pieces and spots outside the trim", () => {
    expect(splitAt(wholeClip(D), 4000, TRIM)).toEqual([p(0, 4000), p(4000, D)]);
    expect(splitAt([p(0, 4000), p(4000, D)], 4200, TRIM)).toBeNull();
    expect(splitAt(wholeClip(D), 500, TRIM)).toBeNull();
    expect(splitAt(wholeClip(D), 9500, TRIM)).toBeNull();
  });

  it("saves the removed pieces as ranges", () => {
    const split = splitAt(splitAt(wholeClip(D), 4000, TRIM)!, 6000, TRIM)!;
    expect(removedRanges(toggle(split, 1), TRIM)).toEqual([{ startMs: 4000, endMs: 6000 }]);
    expect(removedRanges(split, TRIM)).toEqual([]);
  });

  it("hides what the trim covers without forgetting it", () => {
    expect(removedRanges(middleOff, { start: 5000, end: 9000 })).toEqual([{ startMs: 5000, endMs: 6000 }]);
    expect(removedRanges(middleOff, { start: 7000, end: 9000 })).toEqual([]);
    expect(removedRanges(middleOff, TRIM)).toEqual([{ startMs: 4000, endMs: 6000 }]);
  });

  it("merges adjacent removed pieces when saving", () => {
    const pieces = [p(0, 2000), p(2000, 3000, true), p(3000, 5000, true), p(5000, D)];
    expect(removedRanges(pieces, TRIM)).toEqual([{ startMs: 2000, endMs: 5000 }]);
  });

  it("deleting a split keeps the merged piece unless both were removed", () => {
    expect(deleteSplit(middleOff, 0)).toEqual([p(0, 6000), p(6000, D)]);
    expect(deleteSplit([p(0, 4000, true), p(4000, D, true)], 0)).toEqual([p(0, D, true)]);
  });

  it("moving a split keeps both neighbours at least 0.5 s long", () => {
    expect(moveSplit(middleOff, 0, 5000)).toEqual([p(0, 5000), p(5000, 6000, true), p(6000, D)]);
    expect(moveSplit(middleOff, 0, 5900)[0].end).toBe(5500);
    expect(moveSplit(middleOff, 0, 100)[0].end).toBe(500);
  });

  it("never lets every kept part inside the trim go", () => {
    const two = [p(0, 4000), p(4000, D)];
    expect(canRemove(two, 0, TRIM)).toBe(true);
    expect(canRemove(toggle(two, 1), 0, TRIM)).toBe(false);
    // The kept piece left of the trim doesn't count: it is hidden.
    expect(canRemove([p(0, 1500), p(1500, D)], 1, { start: 2000, end: 9000 })).toBe(false);
  });

  it("rebuilds the pieces from saved ranges", () => {
    const pieces = piecesFromRanges([{ startMs: 2000, endMs: 3000 }], D);
    expect(pieces).toEqual([p(0, 2000), p(2000, 3000, true), p(3000, D)]);
    expect(removedRanges(pieces, TRIM)).toEqual([{ startMs: 2000, endMs: 3000 }]);
    expect(piecesFromRanges([], D)).toEqual(wholeClip(D));
  });

  it("knows the kept time", () => {
    expect(keptRanges(middleOff, TRIM)).toEqual([
      { start: 1000, end: 4000 },
      { start: 6000, end: 9000 },
    ]);
    expect(keptLength(middleOff, TRIM)).toBe(6000);
  });

  it("plays from the next kept time and loops", () => {
    const kept = [
      { start: 1000, end: 2000 },
      { start: 3000, end: 9000 },
    ];
    expect(playFrom(1500, kept)).toBe(1500);
    expect(playFrom(2500, kept)).toBe(3000);
    expect(playFrom(9500, kept)).toBe(1000);
    expect(playFrom(500, kept)).toBe(1000);
  });

  it("keeps the thumbnail on a kept frame", () => {
    const kept = [
      { start: 1000, end: 2000 },
      { start: 3000, end: 9000 },
    ];
    expect(clampToKept(1500, kept)).toBe(1500);
    expect(clampToKept(2500, kept)).toBe(3000);
    expect(clampToKept(9500, kept)).toBe(8999);
  });

  it("compares ranges", () => {
    expect(sameRanges([{ startMs: 1, endMs: 2 }], [{ startMs: 1, endMs: 2 }])).toBe(true);
    expect(sameRanges([{ startMs: 1, endMs: 2 }], [])).toBe(false);
  });
});

describe("editedLength", () => {
  it("is the whole clip with no edit", () => {
    expect(editedLength({ trimStartMs: null, trimEndMs: null, removedRanges: [] }, D)).toBe(D);
  });
  it("takes the trim and the removed parts off", () => {
    const edit = { trimStartMs: 1000, trimEndMs: 9000, removedRanges: [{ startMs: 3000, endMs: 5000 }] };
    expect(editedLength(edit, D)).toBe(6000);
  });
  it("ignores removed parts outside the trim and a trim past the end", () => {
    const edit = { trimStartMs: 2000, trimEndMs: 20_000, removedRanges: [{ startMs: 0, endMs: 1000 }] };
    expect(editedLength(edit, D)).toBe(8000);
  });
});
