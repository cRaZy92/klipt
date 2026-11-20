// `use:stickyTop` on a page's pinned block (.page-top): keeps `--page-top-h` on its page set to
// the block's height, so things pinned below it (the day headings) stop right under it.
export function stickyTop(node: HTMLElement) {
  const page = node.parentElement;
  if (!page) return;
  const observer = new ResizeObserver(() => page.style.setProperty("--page-top-h", `${node.offsetHeight}px`));
  observer.observe(node);
  return {
    destroy() {
      observer.disconnect();
      page.style.removeProperty("--page-top-h");
    },
  };
}
