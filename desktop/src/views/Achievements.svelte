<script lang="ts">
  import { untrack } from "svelte";
  import { ACHIEVEMENTS, type AchievementDef, type EarnedAchievement, type MyAchievements } from "@klipt/shared";
  import { api, errorCode, errorMessage } from "../lib/api";
  import { AchievementTips } from "../lib/achievementTip.svelte";
  import AchievementCard from "../components/AchievementCard.svelte";
  import Icon from "../components/Icon.svelte";

  // The badges you've collected. Locked ones are dimmed; hovering any shows its full card (what it's
  // for, what it unlocks, when you got it). Secret ones stay ??? until earned.
  let {
    active = true,
    name = "?",
    avatarUrl = null,
    onunauthorized,
  }: { active?: boolean; name?: string; avatarUrl?: string | null; onunauthorized: () => void } = $props();

  const tips = new AchievementTips();
  // A hidden screen shows no card.
  $effect(() => {
    if (!active) tips.hide();
  });

  let mine = $state<MyAchievements | null>(null);
  let loadError = $state<string | null>(null);

  async function load() {
    try {
      mine = await api.myAchievements();
      loadError = null;
    } catch (e) {
      if (errorCode(e) === "unauthorized") return onunauthorized();
      loadError = errorMessage(e);
    }
  }

  // Again each time the screen comes back: achievements are earned in the background.
  $effect(() => {
    if (active) untrack(() => void load());
  });

  const earned = $derived(new Map((mine?.earned ?? []).map((e) => [e.id, e] as [string, EarnedAchievement])));
  const CATEGORIES: { id: AchievementDef["category"]; label: string }[] = [
    { id: "milestones", label: "Milestones" },
    { id: "social", label: "With friends" },
    { id: "editor", label: "Editor" },
    { id: "hidden", label: "Secret" },
  ];
  const hiddenLeft = $derived(ACHIEVEMENTS.filter((a) => a.hidden && !earned.has(a.id)).length);
  const progress = $derived(mine ? mine.earned.length / ACHIEVEMENTS.length : 0);

  function dateOf(iso: string): string {
    return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  }
</script>

<section class="page achievements">
  <header>
    <h1>Achievements</h1>
    <p class="muted">Earned by clipping, editing and sharing. Some unlock new looks in Appearance.</p>
    <div class="tally" aria-label={mine ? `${mine.earned.length} of ${ACHIEVEMENTS.length} achievements` : undefined}>
      <div class="progress" style:--value={progress}><div></div></div>
      <span class="count">{mine ? `${mine.earned.length} of ${ACHIEVEMENTS.length}` : "…"}</span>
    </div>
  </header>

  {#if loadError}
    <p class="error" role="alert">{loadError} <button class="btn btn-sm" onclick={load}>Try again</button></p>
  {/if}

  {#each CATEGORIES as cat (cat.id)}
    <section class="group">
      <h2>{cat.label}</h2>
      <ul class="badges">
        {#each ACHIEVEMENTS.filter((a) => a.category === cat.id && (!a.hidden || earned.has(a.id))) as a (a.id)}
          {@const got = earned.get(a.id)}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex (focusable so the card shows from the keyboard too) -->
          <li class="badge" class:earned={!!got} class:locked={!got} tabindex="0" use:tips.trigger={{ achievement: a, earnedAt: got?.earnedAt }}>
            <span class="medal" aria-hidden="true">
              <span class="glyph">{a.icon}</span>
              {#if !got}<span class="lock-pin"><Icon name="lock" size={11} /></span>{/if}
            </span>
            <span class="text">
              <strong>{a.name}</strong>
              <small>{a.description}</small>
              {#if got}<small class="when">{dateOf(got.earnedAt)}</small>{/if}
            </span>
          </li>
        {/each}
        {#if cat.id === "hidden"}
          {#each Array.from({ length: hiddenLeft }) as _, i (i)}
            <li class="badge secret">
              <span class="medal" aria-hidden="true"><Icon name="lock" size={20} /></span>
              <span class="text"><strong>???</strong><small>A secret. Keep clipping.</small></span>
            </li>
          {/each}
        {/if}
      </ul>
      {#if cat.id === "hidden" && hiddenLeft > 0}<p class="muted">+{hiddenLeft} hidden</p>{/if}
    </section>
  {/each}
</section>

<AchievementCard tip={tips.current} {name} {avatarUrl} onclose={tips.hide} />

<style>
  header h1 {
    margin: 0 0 4px;
  }
  .muted {
    color: var(--text-muted);
    margin: 0;
  }
  .group > .muted {
    margin-top: 0.5rem;
  }
  .tally {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    max-width: 26rem;
    margin-top: 0.9rem;
  }
  .tally .progress {
    flex: 1;
  }
  .count {
    color: var(--text-muted);
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
  }
  .group h2 {
    margin: 0 0 0.75rem;
    font-size: 1.05rem;
  }
  .badges {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
    gap: 0.6rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .badge {
    position: relative;
    display: flex;
    gap: 0.75rem;
    align-items: center;
    padding: 0.7rem 0.85rem;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--border);
  }
  .badge.earned {
    background: linear-gradient(135deg, var(--accent-softer), var(--surface) 60%);
    box-shadow: inset 0 0 0 1px var(--accent-soft);
  }
  .badge.locked,
  .badge.secret {
    background: color-mix(in srgb, var(--surface) 55%, transparent);
  }
  .badge.locked > .text,
  .badge.secret > .text {
    opacity: 0.6;
  }
  .badge {
    cursor: default;
    outline: none;
    transition: box-shadow 0.15s;
  }
  .badge.locked:hover {
    box-shadow: inset 0 0 0 1px var(--border-strong);
  }
  .badge.earned:hover {
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .badge:focus-visible {
    box-shadow: 0 0 0 2px var(--accent);
  }
  .medal {
    position: relative;
    display: grid;
    place-items: center;
    flex: none;
    width: 2.6rem;
    height: 2.6rem;
    border-radius: 50%;
    background: var(--surface-2);
    font-size: 1.35rem;
  }
  .badge.locked .glyph {
    filter: grayscale(1);
    opacity: 0.45;
  }
  .badge.secret .medal {
    color: var(--text-faint);
    box-shadow: inset 0 0 0 1px var(--border);
  }
  /* The small padlock on a locked badge's medal. */
  .lock-pin {
    position: absolute;
    right: -0.2rem;
    bottom: -0.2rem;
    display: grid;
    place-items: center;
    width: 1.2rem;
    height: 1.2rem;
    border: 2px solid var(--surface);
    border-radius: 50%;
    background: var(--surface-3);
    color: var(--text-muted);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }
  .text small {
    color: var(--text-muted);
  }
  .text .when {
    color: var(--text-faint);
  }
</style>
