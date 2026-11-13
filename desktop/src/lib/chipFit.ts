// How many game chips fit on one line, so the rest can go behind a "+N more" chip.

export interface ChipRow {
  /** Width of each game chip, in order. */
  widths: number[];
  /** Width of the "All games" chip that always comes first. */
  allWidth: number;
  /** Width of the "+N more" chip. */
  moreWidth: number;
  gap: number;
  available: number;
  /** Index of the selected game, or -1. A hidden selection is pinned next to "+N more". */
  selected: number;
}

/** How many games (from the start) to show; all of them when everything fits on the line. */
export function fitChips({ widths, allWidth, moreWidth, gap, available, selected }: ChipRow): number {
  const total = widths.reduce((sum, w) => sum + gap + w, allWidth);
  if (total <= available) return widths.length;

  let used = allWidth;
  let count = 0;
  for (;;) {
    if (count >= widths.length) return widths.length;
    const next = used + gap + widths[count];
    const pinned = selected > count ? gap + widths[selected] : 0;
    if (next + pinned + gap + moreWidth > available) return count;
    used = next;
    count++;
  }
}
