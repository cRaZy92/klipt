// Selecting several clips in My clips: plain click toggles one, Shift+click selects the run from
// the last clicked one, everything in the order the cards are shown.

/** The new selection after a click on `key`. `order` is the shown order; `anchor` the last click. */
export function selectClick(
  current: ReadonlySet<string>,
  order: readonly string[],
  key: string,
  anchor: string | null,
  shift: boolean,
): Set<string> {
  const next = new Set(current);
  const from = anchor === null ? -1 : order.indexOf(anchor);
  const to = order.indexOf(key);
  if (shift && from >= 0 && to >= 0) {
    // Like a file manager: the run takes the anchor's state (selecting or clearing).
    const select = current.has(anchor!);
    const [a, b] = from < to ? [from, to] : [to, from];
    for (const k of order.slice(a, b + 1)) {
      if (select) next.add(k);
      else next.delete(k);
    }
    return next;
  }
  if (next.has(key)) next.delete(key);
  else next.add(key);
  return next;
}

/** Drops what is no longer shown (filtered out or deleted); the same set when nothing changes. */
export function keepShown(current: Set<string>, order: readonly string[]): Set<string> {
  const shown = new Set(order);
  const kept = [...current].filter((k) => shown.has(k));
  return kept.length === current.size ? current : new Set(kept);
}
