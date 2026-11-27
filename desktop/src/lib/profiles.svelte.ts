// People's profiles: opening one (App.svelte decides how), and the hover card shown over any
// name or picture marked with `use:userHover`. Profiles are kept a minute, so moving between the
// cards of one friend doesn't ask the server each time. Cleared on logout.

import { api, type UserProfile } from "./api";

const FRESH_MS = 60_000;
const SHOW_AFTER_MS = 350;
const HIDE_AFTER_MS = 200;

const loaded = new Map<string, { at: number; profile: Promise<UserProfile> }>();

/** `username`'s profile: the kept one while fresh (or `fresh` false), else asked again. */
export function loadProfile(username: string, fresh = false): Promise<UserProfile> {
  const key = username.toLowerCase();
  const kept = loaded.get(key);
  if (kept && !fresh && Date.now() - kept.at < FRESH_MS) return kept.profile;
  const profile = api.userProfile(username);
  loaded.set(key, { at: Date.now(), profile });
  profile.catch(() => {
    if (loaded.get(key)?.profile === profile) loaded.delete(key);
  });
  return profile;
}

/** Forgets every kept profile, e.g. on logout or after a friend change. */
export function forgetProfiles() {
  loaded.clear();
}

let opener: ((username: string) => void) | null = null;

/** App.svelte: how a profile opens. */
export function setProfileOpener(open: ((username: string) => void) | null) {
  opener = open;
}

/** Opens `username`'s profile screen. */
export function openProfile(username: string) {
  hideCard();
  opener?.(username);
}

/** The hover card on screen: whose, and the box of what it points at. */
export const hoverCard = $state<{ profile: UserProfile | null; anchor: DOMRect | null }>({ profile: null, anchor: null });

let showTimer: ReturnType<typeof setTimeout> | undefined;
let hideTimer: ReturnType<typeof setTimeout> | undefined;
/** The element the pointer is on (or last was), so a late answer for another one is dropped. */
let pointing: HTMLElement | null = null;

export function hideCard() {
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
  pointing = null;
  hoverCard.profile = null;
  hoverCard.anchor = null;
}

/** The pointer reached the card itself: keep it. */
export function holdCard() {
  clearTimeout(hideTimer);
}

/** The pointer left the card (or what it points at): hide it soon, unless it comes back. */
export function releaseCard() {
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
  hideTimer = setTimeout(hideCard, HIDE_AFTER_MS);
}

/**
 * `use:userHover={username}`: pointing at the element shows that person's hover card after a
 * moment. No username (e.g. yourself on a list that hides you) does nothing.
 */
export function userHover(node: HTMLElement, username: string | null | undefined) {
  let name = username;
  const enter = () => {
    if (!name) return;
    clearTimeout(hideTimer);
    if (pointing === node && hoverCard.profile) return;
    clearTimeout(showTimer);
    pointing = node;
    const who = name;
    showTimer = setTimeout(() => {
      loadProfile(who).then(
        (profile) => {
          if (pointing !== node || !node.isConnected) return;
          hoverCard.anchor = node.getBoundingClientRect();
          hoverCard.profile = profile;
        },
        () => {}, // an older server without profiles, or offline: no card
      );
    }, SHOW_AFTER_MS);
  };
  const leave = () => {
    if (pointing === node) releaseCard();
  };
  node.addEventListener("mouseenter", enter);
  node.addEventListener("mouseleave", leave);
  node.addEventListener("mousedown", hideCard);
  return {
    update(next: string | null | undefined) {
      name = next;
    },
    destroy() {
      node.removeEventListener("mouseenter", enter);
      node.removeEventListener("mouseleave", leave);
      node.removeEventListener("mousedown", hideCard);
      if (pointing === node) hideCard();
    },
  };
}
