// Your accent and theme: set as CSS tokens on the page, so everything follows.
import { DEFAULT_LOOKS, looksTokens, type Looks } from "@klipt/shared";

type Root = { style: { setProperty(name: string, value: string): void } };

/** Sets every token of `looks` (the defaults without any) on `root` (the document by default). */
export function applyLooks(looks: Looks | null | undefined, root: Root = document.documentElement): void {
  for (const [name, value] of Object.entries(looksTokens(looks ?? DEFAULT_LOOKS))) root.style.setProperty(name, value);
}

/** Whether an avatar shows its picture or the initial (no picture, or it failed to load). */
export function avatarState(url: string | null | undefined, failed: boolean): "image" | "initial" {
  return url && !failed ? "image" : "initial";
}
