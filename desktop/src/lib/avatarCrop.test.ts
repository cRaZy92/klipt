import { describe, expect, it } from "vitest";
import { cropSquare, panBy, zoomTo } from "./avatarCrop";

const img = { w: 2000, h: 1000 };

describe("avatar crop", () => {
  it("starts on the centred square of the shorter side", () => {
    expect(cropSquare(img, 1, { x: 1000, y: 500 })).toEqual({ sx: 500, sy: 0, size: 1000 });
  });

  it("zooming shrinks the square around its centre", () => {
    expect(cropSquare(img, 2, { x: 1000, y: 500 })).toEqual({ sx: 750, sy: 250, size: 500 });
  });

  it("keeps the square inside the picture", () => {
    const far = cropSquare(img, 1, { x: 5000, y: -300 });
    expect(far.sx + far.size).toBe(2000);
    expect(far.sy).toBe(0);
  });

  it("dragging right shows more of the left, in picture pixels", () => {
    // A 280 px view of a 1000 px square: 28 px of drag is 100 picture pixels.
    expect(panBy(img, 1, { x: 1000, y: 500 }, 28, 0, 280)).toEqual({ x: 900, y: 500 });
    // ...and never past the edge.
    expect(panBy(img, 1, { x: 1000, y: 500 }, 10_000, 0, 280)).toEqual({ x: 500, y: 500 });
  });

  it("zoom stays between 1 and 4 and the centre stays valid", () => {
    expect(zoomTo(img, 9, { x: 1990, y: 990 })).toEqual({ zoom: 4, center: { x: 1875, y: 875 } });
    expect(zoomTo(img, 0.2, { x: 1000, y: 500 }).zoom).toBe(1);
  });
});
