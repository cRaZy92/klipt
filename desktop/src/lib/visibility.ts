import type { Visibility } from "./api";

/** The three levels, in the order the app offers them. The link works for anyone in all three. */
export const VISIBILITY: readonly { value: Visibility; label: string; hint: string; icon: "link" | "users" | "globe" }[] = [
  { value: "link", label: "Link only", hint: "Only people you send the link to can watch it.", icon: "link" },
  { value: "friends", label: "Friends", hint: "Your friends see it in their feed and on your profile.", icon: "users" },
  { value: "public", label: "Public", hint: "Anyone can find it on your profile. Friends see it in the feed.", icon: "globe" },
];

/** A stored or sent value as one of the three; older versions and servers said "feed" for Friends. */
export function toVisibility(v: string | null | undefined): Visibility | null {
  if (v === "feed" || v === "friends") return "friends";
  if (v === "public" || v === "link") return v;
  return null;
}

export const visibilityLabel = (v: string | null | undefined): string =>
  VISIBILITY.find((x) => x.value === toVisibility(v))?.label ?? "";

export function nextVisibilityToast(v: Visibility): string {
  return v === "link" ? "Now link-only." : v === "friends" ? "Now friends only." : "Now public.";
}
