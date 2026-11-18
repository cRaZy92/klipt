import { describe, expect, it } from "vitest";
import type { ZoomRange } from "./api";
import {
  addZoomAt,
  boxFromRect,
  boxRect,
  moveZoom,
  removeZoom,
  resizeZoom,
  sameZooms,
  setZoomBox,
  zoomAt,
  zoomIndexAt,
} from "./zoom";

const z = (startMs: number, endMs: number, zoom = 2, x = 0.5, y = 0.5): ZoomRange => ({ startMs, endMs, zoom, x, y });
const TRIM = { start: 0, end: 20_000 };

describe("zoomAt", () => {
  const zooms = [z(1000, 3000)];

  it("eases in, holds and eases out", () => {
    expect(zoomAt(zooms, 999).scale).toBe(1);
    expect(zoomAt(zooms, 3000).scale).toBe(1);
    expect(zoomAt(zooms, 2000).scale).toBe(2);
    const rising = zoomAt(zooms, 1200).scale;
    expect(rising).toBeGreaterThan(1);
    expect(rising).toBeLessThan(2);
    expect(zoomAt(zooms, 2800).scale).toBe(rising);
  });

  it("is centred and unzoomed outside every zoom", () => {
    expect(zoomAt(zooms, 5000)).toEqual({ scale: 1, x: 0.5, y: 0.5 });
    expect(zoomAt([z(1000, 3000, 3, 1, 0)], 2000)).toEqual({ scale: 3, x: 1, y: 0 });
  });

  it("ramps over half of a short zoom", () => {
    expect(zoomAt([z(1000, 1600)], 1300).scale).toBe(2);
    expect(zoomAt([z(1000, 1600)], 1299).scale).toBeLessThan(2);
  });

  it("finds the zoom playing", () => {
    expect(zoomIndexAt([z(1000, 2000), z(2000, 3000)], 2000)).toBe(1);
    expect(zoomIndexAt([z(1000, 2000)], 2000)).toBe(-1);
  });
});

describe("addZoomAt", () => {
  it("adds a 3 s 2× centred zoom at the playhead", () => {
    expect(addZoomAt([], 5000, TRIM)).toEqual([z(5000, 8000)]);
  });

  it("is cut short to fit before the next zoom or the trim end", () => {
    expect(addZoomAt([z(6000, 9000)], 5000, TRIM)).toEqual([z(5000, 6000), z(6000, 9000)]);
    expect(addZoomAt([], 19_000, TRIM)).toEqual([z(19_000, 20_000)]);
  });

  it("keeps the list sorted", () => {
    expect(addZoomAt([z(9000, 10_000)], 1000, TRIM)?.map((r) => r.startMs)).toEqual([1000, 9000]);
  });

  it("refuses without room", () => {
    expect(addZoomAt([z(6000, 9000)], 5800, TRIM)).toBeNull();
    expect(addZoomAt([z(6000, 9000)], 7000, TRIM)).toBeNull();
    expect(addZoomAt([], 19_700, TRIM)).toBeNull();
    expect(addZoomAt([], 500, { start: 1000, end: 9000 })).toBeNull();
  });

  it("may start where another ends", () => {
    expect(addZoomAt([z(1000, 3000)], 3000, TRIM)).toEqual([z(1000, 3000), z(3000, 6000)]);
  });
});

describe("moveZoom", () => {
  const zooms = [z(1000, 2000), z(3000, 4000), z(6000, 7000)];

  it("keeps the length", () => {
    expect(moveZoom(zooms, 1, 4500, TRIM)[1]).toEqual(z(4500, 5500));
  });

  it("can't pass a neighbour or the trim", () => {
    expect(moveZoom(zooms, 1, 1500, TRIM)[1]).toEqual(z(2000, 3000));
    expect(moveZoom(zooms, 1, 9000, TRIM)[1]).toEqual(z(5000, 6000));
    expect(moveZoom([z(1000, 2000)], 0, -500, { start: 500, end: 9000 })[0]).toEqual(z(500, 1500));
    expect(moveZoom([z(1000, 2000)], 0, 9000, { start: 0, end: 9000 })[0]).toEqual(z(8000, 9000));
  });

  it("returns the same list when nothing moves", () => {
    expect(moveZoom(zooms, 1, 3000, TRIM)).toBe(zooms);
  });
});

describe("resizeZoom", () => {
  const zooms = [z(1000, 2000), z(3000, 4000), z(6000, 7000)];

  it("moves one edge", () => {
    expect(resizeZoom(zooms, 1, "start", 2500, TRIM)[1]).toEqual(z(2500, 4000));
    expect(resizeZoom(zooms, 1, "end", 5000, TRIM)[1]).toEqual(z(3000, 5000));
  });

  it("stays at least 500 ms long and between its neighbours", () => {
    expect(resizeZoom(zooms, 1, "start", 3900, TRIM)[1]).toEqual(z(3500, 4000));
    expect(resizeZoom(zooms, 1, "end", 3100, TRIM)[1]).toEqual(z(3000, 3500));
    expect(resizeZoom(zooms, 1, "start", 0, TRIM)[1]).toEqual(z(2000, 4000));
    expect(resizeZoom(zooms, 1, "end", 9000, TRIM)[1]).toEqual(z(3000, 6000));
  });
});

describe("the zoom's box", () => {
  it("clamps the zoom and the position", () => {
    const zooms = [z(1000, 2000)];
    expect(setZoomBox(zooms, 0, { zoom: 5, x: 1.3, y: -1 })[0]).toEqual(z(1000, 2000, 4, 1, 0));
    expect(setZoomBox(zooms, 0, { zoom: 1, x: 0.2, y: 0.3 })[0]).toEqual(z(1000, 2000, 1.25, 0.2, 0.3));
  });

  it("maps to a rectangle and back", () => {
    expect(boxRect({ zoom: 2, x: 1, y: 0 })).toEqual({ left: 0.5, top: 0, width: 0.5, height: 0.5 });
    expect(boxRect({ zoom: 4, x: 0.5, y: 0.5 })).toEqual({ left: 0.375, top: 0.375, width: 0.25, height: 0.25 });
    for (const box of [{ zoom: 2, x: 1, y: 0 }, { zoom: 3, x: 0.25, y: 0.8 }, { zoom: 1.25, x: 0, y: 1 }]) {
      const back = boxFromRect(boxRect(box));
      expect(back.zoom).toBeCloseTo(box.zoom);
      expect(back.x).toBeCloseTo(box.x);
      expect(back.y).toBeCloseTo(box.y);
    }
  });

  it("keeps a box drawn too big or too small in range", () => {
    expect(boxFromRect({ left: 0, top: 0, width: 1 }).zoom).toBe(1.25);
    expect(boxFromRect({ left: 0.9, top: 0.9, width: 0.1 })).toEqual({ zoom: 4, x: 1, y: 1 });
  });
});

describe("lists", () => {
  it("removes a zoom and compares lists", () => {
    const zooms = [z(1000, 2000), z(3000, 4000)];
    expect(removeZoom(zooms, 0)).toEqual([z(3000, 4000)]);
    expect(sameZooms(zooms, [z(1000, 2000), z(3000, 4000)])).toBe(true);
    expect(sameZooms(zooms, [z(1000, 2000), z(3000, 4000, 3)])).toBe(false);
    expect(sameZooms(zooms, [z(1000, 2000)])).toBe(false);
  });
});
