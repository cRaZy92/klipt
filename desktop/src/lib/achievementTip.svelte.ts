import type { AchievementDef } from "@klipt/shared";

/** The achievement card shown while hovering (or focusing) something that names an achievement. */
export interface Tip {
  achievement: AchievementDef;
  /** When it was earned; null while locked. */
  earnedAt: string | null;
  /** The hovered element's box, to place the card next to. */
  anchor: DOMRect;
}

export const TIP_ID = "achievement-tip";

/**
 * One card per page: `use:tips.trigger={…}` on each badge or locked choice, and
 * `<AchievementCard tip={tips.current} />` once. Opens after a short hover so sweeping the mouse
 * across a grid doesn't flash cards, and closes on leave, blur, scroll or Escape.
 */
export class AchievementTips {
  current = $state<Tip | null>(null);
  #timer: ReturnType<typeof setTimeout> | undefined;
  #owner: HTMLElement | null = null;

  hide = () => {
    clearTimeout(this.#timer);
    this.#owner?.removeAttribute("aria-describedby");
    this.#owner = null;
    this.current = null;
  };

  trigger = (node: HTMLElement, initial: { achievement: AchievementDef; earnedAt?: string | null } | null) => {
    let value = initial;
    const open = (delay: number) => {
      clearTimeout(this.#timer);
      if (!value) return;
      this.#timer = setTimeout(() => {
        if (!value || !node.isConnected) return;
        this.#owner?.removeAttribute("aria-describedby");
        this.#owner = node;
        node.setAttribute("aria-describedby", TIP_ID);
        this.current = { achievement: value.achievement, earnedAt: value.earnedAt ?? null, anchor: node.getBoundingClientRect() };
      }, delay);
    };
    const close = () => {
      if (this.#owner === node || !this.current) this.hide();
      else clearTimeout(this.#timer);
    };
    const enter = () => open(this.current ? 0 : 180);
    const focus = () => {
      if (node.matches(":focus-visible")) open(0);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") this.hide();
    };
    node.addEventListener("pointerenter", enter);
    node.addEventListener("pointerleave", close);
    node.addEventListener("focus", focus);
    node.addEventListener("blur", close);
    node.addEventListener("keydown", key);
    return {
      update(next: typeof initial) {
        value = next;
      },
      destroy: () => {
        close();
        node.removeEventListener("pointerenter", enter);
        node.removeEventListener("pointerleave", close);
        node.removeEventListener("focus", focus);
        node.removeEventListener("blur", close);
        node.removeEventListener("keydown", key);
      },
    };
  };
}
