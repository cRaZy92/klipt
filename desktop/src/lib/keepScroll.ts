// Keeps a kept-alive screen's scroll position across being hidden. An engine may reset a hidden
// scroller (its contents aren't laid out while hidden), so the position is saved when it hides
// and put back when it's shown again.

/** Svelte action: `use:keepScroll={shown}`. */
export function keepScroll(node: HTMLElement, shown: boolean) {
  let saved = node.scrollTop;
  return {
    update(next: boolean) {
      if (next === shown) return;
      if (!next) saved = node.scrollTop;
      else node.scrollTop = saved;
      shown = next;
    },
  };
}
