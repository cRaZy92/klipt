// `use:stickyTop` on a page's pinned block (.page-top):
// - keeps `--page-top-h` on its page set to the block's height, so things pinned below it (the day
//   headings) stop right under it;
// - adds `compact` to the block once its screen is scrolled, so a small window keeps room for clips.

/** Scrolled past this, the header goes compact; back above `COMPACT_OFF` it's full again. The gap
 *  keeps it from flickering when shrinking the header itself moves the scroll position. */
export const COMPACT_ON = 48;
export const COMPACT_OFF = 8;

/** Whether the header should be compact at `scrollTop`, given whether it is now. */
export function compactAt(scrollTop: number, compact: boolean): boolean {
  return compact ? scrollTop > COMPACT_OFF : scrollTop > COMPACT_ON;
}

export function stickyTop(node: HTMLElement) {
  const page = node.parentElement;
  if (!page) return;
  const scroller = node.closest<HTMLElement>(".screen");
  const observer = new ResizeObserver(() => page.style.setProperty("--page-top-h", `${node.offsetHeight}px`));
  observer.observe(node);
  let compact = false;
  const onscroll = () => {
    const next = compactAt(scroller!.scrollTop, compact);
    if (next !== compact) node.classList.toggle("compact", (compact = next));
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
