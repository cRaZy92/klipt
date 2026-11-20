// When the Feed, shown again, loads its first page anew. A refresh replaces the list with the
// first page, so once the user loaded older clips it would throw those (and their place) away.

/** Shown again after longer than this, a first-page-only feed refreshes. */
export const FEED_STALE_MS = 30_000;

export interface FeedLoaded {
  /** When the first page was last loaded (ms); 0 = never. */
  loadedAt: number;
  /** Pages shown: 1 = just the first, more once older clips were loaded. */
  pages: number;
}

export function feedNeedsRefresh({ loadedAt, pages }: FeedLoaded, now: number): boolean {
  if (pages === 0) return true;
  if (pages > 1) return false;
  return now - loadedAt > FEED_STALE_MS;
}

/**
 * A friend's clip shared since you last opened the Feed (`seenBefore`, the server's ISO 8601
 * time, which sorts as text). Unknown (`null`): nothing is marked. Your own clips never are.
 */
export function isNewClip(
  clip: { createdAt: string; owner: { username: string } },
  seenBefore: string | null,
  me: string | null,
): boolean {
  if (seenBefore === null) return false;
  if (me && clip.owner.username.toLowerCase() === me.toLowerCase()) return false;
  return clip.createdAt > seenBefore;
}
