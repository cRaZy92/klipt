import { describe, expect, it } from "vitest";
import {
  clampView,
  followView,
  fullView,
  inView,
  isZoomed,
  maxScale,
  peakWindow,
  panView,
  scaleToSlider,
  scrollThumb,
  sliderToScale,
  viewAt,
  viewMs,
  viewPct,
  viewScale,
  viewSpan,
  zoomView,
} from "./timelineView";

const D = 60_000;

describe("zoom levels", () => {
  it("zooms in down to a 1 s view", () => {
    expect(maxScale(D)).toBe(60);
    expect(maxScale(800)).toBe(1);
    expect(maxScale(0)).toBe(1);
  });

  it("knows how far a view is zoomed in", () => {
    expect(viewScale(fullView(D), D)).toBe(1);
    expect(viewScale({ from: 0, to: 15_000 }, D)).toBe(4);
    expect(isZoomed(fullView(D), D)).toBe(false);
    expect(isZoomed({ from: 1, to: D }, D)).toBe(true);
  });
});

describe("clampView", () => {
  it("returns the same view when it is valid", () => {
    const v = { from: 1000, to: 5000 };
    expect(clampView(v, D)).toBe(v);
  });

  it("keeps the view inside the clip and at least 1 s long", () => {
    expect(clampView({ from: -500, to: 2000 }, D)).toEqual({ from: 0, to: 2500 });
    expect(clampView({ from: 59_000, to: 62_000 }, D)).toEqual({ from: 57_000, to: 60_000 });
    expect(clampView({ from: 3000, to: 3200 }, D)).toEqual({ from: 3000, to: 4000 });
    expect(clampView({ from: -10, to: 90_000 }, D)).toEqual(fullView(D));
  });

  it("shows a clip shorter than 1 s whole", () => {
    expect(clampView({ from: 100, to: 200 }, 600)).toEqual({ from: 0, to: 600 });
  });
});

describe("zoomView", () => {
  it("keeps the anchor where it is on screen", () => {
    const v = zoomView(fullView(D), D, 4, 30_000);
    expect(v).toEqual({ from: 22_500, to: 37_500 });
    const w = zoomView(v, D, 8, 25_000);
    expect(viewPct(25_000, w)).toBeCloseTo(viewPct(25_000, v));
  });

  it("stops at the clip's edges", () => {
    expect(zoomView(fullView(D), D, 2, 0)).toEqual({ from: 0, to: 30_000 });
    expect(zoomView({ from: 50_000, to: 60_000 }, D, 1, 55_000)).toEqual(fullView(D));
  });

  it("clamps the scale between the whole clip and 1 s", () => {
    expect(zoomView(fullView(D), D, 1000, 0)).toEqual({ from: 0, to: 1000 });
    expect(zoomView({ from: 0, to: 1000 }, D, 0.2, 0)).toEqual(fullView(D));
  });
});

describe("panning", () => {
  it("moves the view, kept inside the clip", () => {
    const v = { from: 10_000, to: 20_000 };
    expect(panView(v, D, 5000)).toEqual({ from: 15_000, to: 25_000 });
    expect(panView(v, D, -50_000)).toEqual({ from: 0, to: 10_000 });
    expect(panView(v, D, 0)).toBe(v);
    expect(viewAt(v, D, 55_000)).toEqual({ from: 50_000, to: 60_000 });
  });
});

describe("followView", () => {
  const v = { from: 10_000, to: 20_000 };

  it("leaves the view alone while the playhead is on screen", () => {
    expect(followView(v, D, 15_000)).toBe(v);
    expect(followView(v, D, 20_000)).toBe(v);
  });

  it("turns the page when the playhead runs off the right edge", () => {
    expect(followView(v, D, 20_100)).toEqual({ from: 19_100, to: 29_100 });
  });

  it("goes back a page when it steps off the left edge", () => {
    expect(followView(v, D, 9_900)).toEqual({ from: 900, to: 10_900 });
  });

  it("centres a jump further than a page away", () => {
    expect(followView(v, D, 45_000)).toEqual({ from: 40_000, to: 50_000 });
    expect(followView(v, D, 0)).toEqual({ from: 0, to: 10_000 });
  });
});

describe("placing things on the view", () => {
  const v = { from: 10_000, to: 20_000 };

  it("maps time to percent and pixels back to time", () => {
    expect(viewPct(15_000, v)).toBe(50);
    expect(viewPct(5_000, v)).toBe(-50);
    expect(viewMs(250, 1000, v)).toBe(12_500);
    expect(viewMs(-50, 1000, v)).toBe(10_000);
    expect(viewMs(5000, 1000, v)).toBe(20_000);
  });

  it("never rounds past an edge that isn't a whole ms", () => {
    const f = { from: 66_388.775, to: 72_388.775 };
    expect(viewMs(1000, 1000, f)).toBe(72_388);
    expect(viewMs(0, 1000, f)).toBe(66_389);
    expect(followView(f, D * 2, viewMs(1000, 1000, f))).toBe(f);
  });

  it("matches the whole-clip math at full view", () => {
    expect(viewPct(15_000, fullView(D))).toBe(25);
    expect(viewMs(500, 1000, fullView(D))).toBe(30_000);
  });

  it("clips a stretch to the screen", () => {
    expect(viewSpan(5_000, 12_000, v)).toEqual({ left: 0, width: 20 });
    expect(viewSpan(18_000, 30_000, v)).toEqual({ left: 80, width: 20 });
    expect(viewSpan(21_000, 30_000, v)).toBeNull();
    expect(inView(10_000, v)).toBe(true);
    expect(inView(9_999, v)).toBe(false);
  });

  it("handles a clip with no length", () => {
    expect(viewPct(100, fullView(0))).toBe(0);
    expect(viewMs(100, 1000, fullView(0))).toBe(0);
  });
});

describe("peakWindow", () => {
  const hour = 3_600_000;

  it("is not needed while the whole track's peaks are fine enough", () => {
    expect(peakWindow(fullView(hour), hour, 2 ** 18)).toBeNull();
    expect(peakWindow({ from: 0, to: 1000 }, 60_000, 2 ** 18)).toBeNull();
  });

  it("covers the view with a step either side on a long recording zoomed far in", () => {
    const v = { from: 1_000_500, to: 1_010_500 };
    const w = peakWindow(v, hour, 2 ** 18)!;
    // 10 s rounds up to a 16 384 ms step.
    expect(w).toEqual({ from: 983_040, to: 983_040 + 3 * 16_384 });
    expect(w.from <= v.from && w.to >= v.to).toBe(true);
  });

  it("stays the same while the view pans inside a step", () => {
    const a = peakWindow({ from: 1_000_500, to: 1_010_500 }, hour, 2 ** 18);
    const b = peakWindow({ from: 1_005_000, to: 1_015_000 }, hour, 2 ** 18);
    expect(b).toEqual(a);
  });

  it("stays inside the clip", () => {
    expect(peakWindow({ from: 0, to: 1000 }, hour, 2 ** 18)).toEqual({ from: 0, to: 3072 });
    expect(peakWindow({ from: hour - 1000, to: hour }, hour, 2 ** 18)!.to).toBe(hour);
  });
});

describe("zoom slider and scrollbar", () => {
  it("moves the slider logarithmically", () => {
    expect(scaleToSlider(1, D)).toBe(0);
    expect(scaleToSlider(60, D)).toBeCloseTo(1);
    expect(sliderToScale(0.5, D)).toBeCloseTo(Math.sqrt(60));
    expect(sliderToScale(scaleToSlider(7, D), D)).toBeCloseTo(7);
    expect(scaleToSlider(5, 500)).toBe(0);
  });

  it("sizes the scrollbar thumb to the view", () => {
    expect(scrollThumb({ from: 15_000, to: 30_000 }, D)).toEqual({ left: 25, width: 25 });
    expect(scrollThumb(fullView(0), 0)).toEqual({ left: 0, width: 100 });
  });
});
