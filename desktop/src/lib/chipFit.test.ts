import { describe, expect, it } from "vitest";
import { fitChips } from "./chipFit";

const row = { allWidth: 80, moreWidth: 60, gap: 10, selected: -1 };

describe("fitChips", () => {
  it("shows every game when they all fit", () => {
    expect(fitChips({ ...row, widths: [100, 100], available: 300 })).toBe(2);
  });

  it("leaves room for the more chip when they don't", () => {
    // 80 + (10+100)*2 = 300, + 10+60 = 370 ≤ 400; a third chip would need 480.
    expect(fitChips({ ...row, widths: [100, 100, 100, 100], available: 400 })).toBe(2);
  });

  it("can show no games at all on a very narrow row", () => {
    expect(fitChips({ ...row, widths: [100, 100], available: 160 })).toBe(0);
  });

  it("makes room for a selected game that would be hidden", () => {
    // Without the pin 2 fit; pinning game 3 (100 + gap) costs one slot.
    expect(fitChips({ ...row, widths: [100, 100, 100, 100], available: 400, selected: 3 })).toBe(1);
  });

  it("doesn't reserve a pin for a selected game that is already visible", () => {
    expect(fitChips({ ...row, widths: [100, 100, 100, 100], available: 400, selected: 1 })).toBe(2);
  });
});
