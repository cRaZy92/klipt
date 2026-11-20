// `use:stickyTop` on a page's pinned block (.page-top):
// - keeps `--page-top-h` on its page set to the height the block shows, so things pinned below it
//   (the day headings) stop right under it;
// - adds `compact` to the block once its screen is scrolled, so a small window keeps room for clips.
//
// Compact only moves, scales and fades things (app.css): the block keeps its laid-out size, so
// nothing is laid out again while it animates (with hundreds of cards that was choppy). The
// shifts it needs are measured here and handed to the CSS as variables.

/** Scrolled past this, the header goes compact; back above `COMPACT_OFF` it's full again. The gap
 *  keeps it from flickering at the edge. */
export const COMPACT_ON = 48;
export const COMPACT_OFF = 8;

/** The compact title's size against the full one (1.3rem / 1.85rem). */
export const TITLE_SCALE = 0.7;

/** Whether the header should be compact at `scrollTop`, given whether it is now. */
export function compactAt(scrollTop: number, compact: boolean): boolean {
  return compact ? scrollTop > COMPACT_OFF : scrollTop > COMPACT_ON;
}

export interface CompactShifts {
  /** How far the title block moves up: the top padding that goes. */
  title: number;
  /** How far the page tools move up, to line up with the smaller title. */
  tools: number;
  /** How far the rows under the heading (and the block's own backdrop) move up: the height saved. */
  rest: number;
}

/**
 * The shifts for going compact, in px. `rem` is the root font size, `titleHeight` the full
 * title's height, `subtitleHeight` what the subtitle adds under it (margin included).
 */
export function compactShifts(rem: number, titleHeight: number, subtitleHeight: number): CompactShifts {
  const padding = 0.85 * rem; // top padding 1.75rem → 0.9rem
  const gap = 0.5 * rem; // gap under the heading 1.25rem → 0.75rem
  const tools = padding + titleHeight * (1 - TITLE_SCALE) + subtitleHeight;
  return { title: padding, tools, rest: tools + gap };
}

export function stickyTop(node: HTMLElement) {
  const page = node.parentElement;
  if (!page) return;
  const scroller = node.closest<HTMLElement>(".screen");
  let compact = false;
  let saved = 0;

  function measure() {
    const title = node.querySelector<HTMLElement>(".page-head h1");
    const titleHeight = title?.offsetHeight ?? 0;
    const subtitleHeight = Math.max(0, (title?.parentElement?.offsetHeight ?? 0) - titleHeight);
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    const shifts = compactShifts(rem, titleHeight, subtitleHeight);
    node.style.setProperty("--shift-title", `${shifts.title}px`);
    node.style.setProperty("--shift-tools", `${shifts.tools}px`);
    node.style.setProperty("--shift-rest", `${shifts.rest}px`);
    saved = shifts.rest;
    place();
  }

  function place() {
    page!.style.setProperty("--page-top-h", `${node.offsetHeight - (compact ? saved : 0)}px`);
  }

  const observer = new ResizeObserver(measure);
  observer.observe(node);
  const onscroll = () => {
    const next = compactAt(scroller!.scrollTop, compact);
    if (next === compact) return;
    compact = next;
    node.classList.toggle("compact", compact);
    place();
  };
  scroller?.addEventListener("scroll", onscroll, { passive: true });
  return {
    destroy() {
      observer.disconnect();
      scroller?.removeEventListener("scroll", onscroll);
      page.style.removeProperty("--page-top-h");
    },
  };
}
