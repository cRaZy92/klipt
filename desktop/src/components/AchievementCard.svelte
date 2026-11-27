<script lang="ts">
  import { ACCENTS, BANNERS, FRAMES, THEMES, type CosmeticKind } from "@klipt/shared";
  import { TIP_ID, type Tip } from "../lib/achievementTip.svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  // An achievement in full, floating next to what's hovered: what it's for, what it unlocks (with a
  // preview of the look), and whether you have it. A secret one you haven't earned stays a secret.
  let { tip, name = "?", avatarUrl = null, onclose }: { tip: Tip | null; name?: string; avatarUrl?: string | null; onclose: () => void } = $props();

  const CATEGORY: Record<string, string> = { milestones: "Milestone", social: "With friends", editor: "Editor", hidden: "Secret" };
  const KIND: Record<CosmeticKind, string> = { accent: "Accent colour", theme: "Theme", frame: "Avatar frame", banner: "Profile banner" };

  const a = $derived(tip?.achievement);
  const secret = $derived(!!a?.hidden && !tip?.earnedAt);
  const reward = $derived.by(() => {
    if (!a?.reward) return null;
    const [kind, id] = a.reward.split(":") as [CosmeticKind, string];
    const list = { accent: ACCENTS, theme: THEMES, frame: FRAMES, banner: BANNERS }[kind];
    const item = list.find((c) => c.id === id);
    return item ? { kind, id, name: item.name, item } : null;
  });

  // Placed above the hovered element (below when there's no room), kept inside the window.
  let w = $state(0);
  let h = $state(0);
  const GAP = 8;
  const EDGE = 12;
  const place = $derived.by(() => {
    if (!tip || !w || !h) return null;
    const r = tip.anchor;
    const left = Math.min(Math.max(r.left + r.width / 2 - w / 2, EDGE), window.innerWidth - w - EDGE);
    const above = r.top - h - GAP >= EDGE;
    return { left, top: above ? r.top - h - GAP : r.bottom + GAP, above };
  });

  // The anchor moves away on scroll or resize: close rather than float over the wrong thing.
  $effect(() => {
    if (!tip) return;
    window.addEventListener("scroll", onclose, true);
    window.addEventListener("resize", onclose);
    return () => {
      window.removeEventListener("scroll", onclose, true);
      window.removeEventListener("resize", onclose);
    };
  });

  function dateOf(iso: string): string {
    return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  }
</script>

{#if tip && a}
  <div
    class="card"
    class:earned={!!tip.earnedAt}
    class:below={place && !place.above}
    class:shown={!!place}
    id={TIP_ID}
    role="tooltip"
    bind:offsetWidth={w}
    bind:offsetHeight={h}
    style:left="{place?.left ?? 0}px"
    style:top="{place?.top ?? 0}px"
  >
    <div class="top">
      <span class="medal" aria-hidden="true">
        {#if secret}<Icon name="lock" size={26} />{:else}{a.icon}{/if}
      </span>
      <span class="title">
        <small>{CATEGORY[a.category] ?? a.category}</small>
        <strong>{secret ? "Secret achievement" : a.name}</strong>
      </span>
    </div>

    <div class="body">
      <p>{secret ? "Nobody's saying how. Keep clipping." : `${a.description}.`}</p>

      {#if reward}
        <div class="reward">
          <span class="preview">
            {#if reward.kind === "accent"}
              <span class="swatch" style:--c={ACCENTS.find((x) => x.id === reward.id)?.color}></span>
            {:else if reward.kind === "theme"}
              {@const t = THEMES.find((x) => x.id === reward.id)}
              <span class="theme-chip" style:--b={t?.tokens["--bg"]} style:--s={t?.tokens["--surface-3"]}></span>
            {:else if reward.kind === "frame"}
              <Avatar {name} url={avatarUrl} frame={reward.id} size={30} />
            {:else}
              <span class="banner-chip banner-{reward.id}"></span>
            {/if}
          </span>
          <span class="reward-text">
            <small>{tip.earnedAt ? "Unlocked" : "Unlocks"} · {KIND[reward.kind]}</small>
            <strong>{reward.name}</strong>
          </span>
        </div>
      {/if}
    </div>

    <div class="foot">
      {#if tip.earnedAt}
        <Icon name="check" size={14} /> Earned {dateOf(tip.earnedAt)}
      {:else}
        <Icon name="lock" size={13} /> Not earned yet
      {/if}
    </div>
  </div>
{/if}

<style>
  .card {
    position: fixed;
    z-index: 200;
    width: 18rem;
    overflow: hidden;
    border-radius: var(--radius-lg);
    background: var(--surface-2);
    box-shadow:
      inset 0 0 0 1px var(--border-strong),
      var(--shadow);
    color: var(--text);
    font-size: 0.85rem;
    pointer-events: none;
    visibility: hidden;
  }
  .card.shown {
    visibility: visible;
    animation: pop 0.14s var(--ease-out);
  }
  .card.shown.below {
    animation-name: pop-below;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(4px) scale(0.98);
    }
  }
  @keyframes pop-below {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }

  .top {
    display: flex;
    align-items: center;
    gap: 0.85rem;
    padding: 1rem 1rem 0.9rem;
    background: radial-gradient(14rem 7rem at 0% 0%, color-mix(in srgb, var(--text-faint) 14%, transparent), transparent 75%), var(--surface);
  }
  .earned .top {
    background: radial-gradient(14rem 7rem at 0% 0%, var(--accent-soft), transparent 75%), var(--surface);
  }
  .medal {
    display: grid;
    place-items: center;
    flex: none;
    width: 3.4rem;
    height: 3.4rem;
    border-radius: 50%;
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--border-strong);
    color: var(--text-faint);
    font-size: 1.85rem;
  }
  .earned .medal {
    box-shadow:
      inset 0 0 0 2px var(--accent),
      0 0 18px -4px var(--accent-glow);
  }
  .title {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }
  .title small {
    color: var(--text-faint);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .title strong {
    font-size: 1.05rem;
    line-height: 1.2;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 0.85rem 1rem 0.95rem;
  }
  .body p {
    margin: 0;
    color: var(--text-muted);
  }
  .reward {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    padding: 0.55rem 0.65rem;
    border-radius: var(--radius-sm);
    background: var(--surface-3);
  }
  .preview {
    display: grid;
    place-items: center;
    flex: none;
    min-width: 2.5rem;
  }
  .reward-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .reward-text small {
    color: var(--text-faint);
    font-size: 0.72rem;
  }
  .reward-text strong {
    color: var(--accent-text);
  }
  .swatch {
    width: 1.6rem;
    height: 1.6rem;
    border-radius: 50%;
    background: var(--c);
    box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.15);
  }
  .theme-chip {
    width: 2.4rem;
    height: 1.6rem;
    border-radius: 6px;
    background: linear-gradient(135deg, var(--b) 55%, var(--s) 55%);
    box-shadow: inset 0 0 0 1px var(--s);
  }
  .banner-chip {
    width: 2.5rem;
    height: 1.6rem;
    border-radius: 6px;
    background: var(--banner);
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.6rem 1rem;
    border-top: 1px solid var(--border);
    color: var(--text-faint);
    font-size: 0.78rem;
  }
  .earned .foot {
    color: var(--accent-text);
  }
</style>
