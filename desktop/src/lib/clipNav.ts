// Moving between clips in the player: the Feed's "Up next" and the arrow keys.

/** The clips before and after `id` in the list as shown; null at either end or when it's gone. */
export function neighbors<T extends { id: string }>(list: T[], id: string): { prev: T | null; next: T | null } {
  const i = list.findIndex((c) => c.id === id);
  if (i < 0) return { prev: null, next: null };
  return { prev: list[i - 1] ?? null, next: list[i + 1] ?? null };
}

type KeyLike = Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">;

/** ← and → move between clips; with a modifier they're left to the browser. */
export function navKey(e: KeyLike): "prev" | "next" | null {
  if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return null;
  if (e.key === "ArrowRight") return "next";
  if (e.key === "ArrowLeft") return "prev";
  return null;
}
