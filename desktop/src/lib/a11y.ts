// Keyboard support for popup menus and dialogs: arrow keys in menus, focus kept inside a dialog.

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusables(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.getClientRects().length > 0);
}

/** Where a menu key moves focus among `count` items from `current` (-1: none), or null. */
export function menuTarget(key: string, current: number, count: number): number | null {
  if (count === 0) return null;
  switch (key) {
    case "ArrowDown":
      return current < 0 ? 0 : (current + 1) % count;
    case "ArrowUp":
      return current < 0 ? count - 1 : (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/** Where Tab (or Shift+Tab) wraps to inside a dialog, or null to let the browser move focus. */
export function trapTarget(shift: boolean, current: number, count: number): number | null {
  if (count === 0) return null;
  if (current < 0) return shift ? count - 1 : 0;
  if (shift && current === 0) return count - 1;
  if (!shift && current === count - 1) return 0;
  return null;
}

/**
 * `use:menu={{ close }}` on a `role="menu"` element: focuses its first item, moves with the
 * arrow keys, Home and End, and closes on Escape (focus back on the button that opened it) or Tab.
 */
export function menu(node: HTMLElement, options: { close: () => void }) {
  let { close } = options;
  const opener = document.activeElement as HTMLElement | null;
  const items = () =>
    [...node.querySelectorAll<HTMLElement>('[role="menuitem"]')].filter((el) => !(el as HTMLButtonElement).disabled);
  items()[0]?.focus();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
      opener?.focus();
      return;
    }
    if (e.key === "Tab") {
      close();
      return;
    }
    const list = items();
    const target = menuTarget(e.key, list.indexOf(document.activeElement as HTMLElement), list.length);
    if (target !== null) {
      e.preventDefault();
      list[target].focus();
    }
  }

  node.addEventListener("keydown", onkeydown);
  return {
    update(next: { close: () => void }) {
      close = next.close;
    },
    destroy() {
      node.removeEventListener("keydown", onkeydown);
    },
  };
}

/**
 * `use:trapFocus` on a dialog: moves focus into it (unless something in it already has focus),
 * keeps Tab inside it, and gives focus back to where it was when the dialog closes.
 */
export function trapFocus(node: HTMLElement) {
  const before = document.activeElement as HTMLElement | null;
  // Let the dialog's own content (e.g. an input that focuses itself) go first.
  queueMicrotask(() => {
    if (node.contains(document.activeElement)) return;
    const list = focusables(node);
    const field = list.find((el) => el.matches("input, textarea, select"));
    (field ?? list[0] ?? node).focus();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Tab") return;
    const list = focusables(node);
    const target = trapTarget(e.shiftKey, list.indexOf(document.activeElement as HTMLElement), list.length);
    if (target !== null) {
      e.preventDefault();
      list[target].focus();
    }
  }

  node.addEventListener("keydown", onkeydown);
  return {
    destroy() {
      node.removeEventListener("keydown", onkeydown);
      if (before?.isConnected) before.focus();
    },
  };
}
