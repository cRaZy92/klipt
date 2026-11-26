<script lang="ts">
  import { untrack } from "svelte";
  import {
    ACCENTS,
    ACHIEVEMENTS,
    BANNERS,
    FRAMES,
    THEMES,
    cosmeticName,
    type AchievementDef,
    type CosmeticId,
    type EarnedAchievement,
    type Looks,
    type MyAchievements,
  } from "@klipt/shared";
  import { api, errorCode, errorMessage, type Account } from "../lib/api";
  import { pickProfilePicture } from "../lib/thumbnailImage";
  import { toast } from "../lib/toasts.svelte";
  import Avatar from "../components/Avatar.svelte";
  import AvatarCropDialog from "../components/AvatarCropDialog.svelte";

  // Your profile: picture and banner as friends see them, the badges you've collected, and the
  // looks you can pick (locked ones say which achievement unlocks them).
  let {
    active = true,
    account,
    onaccount,
    onunauthorized,
  }: { active?: boolean; account: Account; onaccount: (a: Account) => void; onunauthorized: () => void } = $props();

  let mine = $state<MyAchievements | null>(null);
  let loadError = $state<string | null>(null);
  let cropping = $state<string | null>(null);
  let busy = $state(false);

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
  const unlocked = $derived(new Set<string>(mine?.unlocked ?? []));
  const looks = $derived<Looks>(mine?.looks ?? account.looks ?? { accent: "klipt", theme: "klipt", frame: null, banner: null });
  const CATEGORIES: { id: AchievementDef["category"]; label: string }[] = [
    { id: "milestones", label: "Milestones" },
    { id: "social", label: "With friends" },
    { id: "editor", label: "Editor" },
    { id: "hidden", label: "Secret" },
  ];
  const hiddenLeft = $derived(ACHIEVEMENTS.filter((a) => a.hidden && !earned.has(a.id)).length);

  function unlocksWith(id: CosmeticId): string {
    const a = ACHIEVEMENTS.find((x) => x.reward === id);
    if (!a) return "Locked";
    return a.hidden ? "Unlocks with a secret achievement" : `Unlocks with ${a.name}`;
  }

  function dateOf(iso: string): string {
    return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  }

  async function equip(change: Partial<Looks>) {
    busy = true;
    try {
      const fresh = await api.setLooks(change);
      onaccount(fresh);
      if (mine && fresh.looks) mine = { ...mine, looks: fresh.looks };
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      busy = false;
    }
  }

  async function choosePicture() {
    try {
      cropping = await pickProfilePicture();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  async function removePicture() {
    busy = true;
    try {
      onaccount(await api.removeAvatar());
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      busy = false;
    }
  }
</script>

<section class="page profile">
  <header class="hero banner-{looks.banner ?? 'none'}" class:has-banner={!!looks.banner}>
    <Avatar name={account.displayName} url={account.avatarUrl} frame={looks.frame} size={88} />
    <div>
      <h1>{account.displayName}</h1>
      {#if account.displayName !== account.username}<p class="handle">@{account.username}</p>{/if}
      <p class="count">
        {mine ? `${mine.earned.length} of ${ACHIEVEMENTS.length} achievements` : "…"}
      </p>
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
          <li class="badge" class:earned={!!got} title={a.reward ? `Unlocks ${cosmeticName(a.reward) ?? a.reward}` : undefined}>
            <span class="medal" aria-hidden="true">{a.icon}</span>
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
              <span class="medal" aria-hidden="true">🔒</span>
              <span class="text"><strong>???</strong><small>A secret. Keep playing.</small></span>
            </li>
          {/each}
        {/if}
      </ul>
      {#if cat.id === "hidden" && hiddenLeft > 0}<p class="muted">+{hiddenLeft} hidden</p>{/if}
    </section>
  {/each}

  <section class="group">
    <h2>Looks</h2>
    <p class="muted">Colours and themes change what you see. Frames and banners are what friends see on you.</p>

    <h3>Picture</h3>
    <div class="row">
      <Avatar name={account.displayName} url={account.avatarUrl} frame={looks.frame} size={56} />
      <button class="btn btn-primary" onclick={choosePicture} disabled={busy}>Change picture</button>
      {#if account.avatarUrl}<button class="btn" onclick={removePicture} disabled={busy}>Remove</button>{/if}
    </div>

    <h3>Accent colour</h3>
    <div class="choices">
      {#each ACCENTS as a (a.id)}
        {@const locked = !unlocked.has(`accent:${a.id}`)}
        <button
          class="choice"
          class:on={looks.accent === a.id}
          disabled={busy || locked}
          title={locked ? unlocksWith(`accent:${a.id}`) : a.name}
          aria-pressed={looks.accent === a.id}
          onclick={() => equip({ accent: a.id })}
        >
          <span class="swatch" style:--c={a.color}></span>
          {a.name}
          {#if locked}<span class="lock" aria-label={unlocksWith(`accent:${a.id}`)}>🔒</span>{/if}
        </button>
      {/each}
    </div>

    <h3>Theme</h3>
    <div class="choices">
      {#each THEMES as t (t.id)}
        {@const locked = !unlocked.has(`theme:${t.id}`)}
        <button
          class="choice"
          class:on={looks.theme === t.id}
          disabled={busy || locked}
          title={locked ? unlocksWith(`theme:${t.id}`) : t.name}
          aria-pressed={looks.theme === t.id}
          onclick={() => equip({ theme: t.id })}
        >
          <span class="theme-chip" style:--b={t.tokens["--bg"]} style:--s={t.tokens["--surface-3"]}></span>
          {t.name}
          {#if locked}<span class="lock" aria-label={unlocksWith(`theme:${t.id}`)}>🔒</span>{/if}
        </button>
      {/each}
    </div>

    <h3>Avatar frame</h3>
    <div class="choices">
      <button class="choice" class:on={!looks.frame} disabled={busy} aria-pressed={!looks.frame} onclick={() => equip({ frame: null })}>None</button>
      {#each FRAMES as f (f.id)}
        {@const locked = !unlocked.has(`frame:${f.id}`)}
        <button
          class="choice"
          class:on={looks.frame === f.id}
          disabled={busy || locked}
          title={locked ? unlocksWith(`frame:${f.id}`) : f.name}
          aria-pressed={looks.frame === f.id}
          onclick={() => equip({ frame: f.id })}
        >
          <Avatar name={account.displayName} url={account.avatarUrl} frame={f.id} size={32} />
          {f.name}
          {#if locked}<span class="lock" aria-label={unlocksWith(`frame:${f.id}`)}>🔒</span>{/if}
        </button>
      {/each}
    </div>

    <h3>Profile banner</h3>
    <div class="choices">
      <button class="choice" class:on={!looks.banner} disabled={busy} aria-pressed={!looks.banner} onclick={() => equip({ banner: null })}>None</button>
      {#each BANNERS as b (b.id)}
        {@const locked = !unlocked.has(`banner:${b.id}`)}
        <button
          class="choice"
          class:on={looks.banner === b.id}
          disabled={busy || locked}
          title={locked ? unlocksWith(`banner:${b.id}`) : b.name}
          aria-pressed={looks.banner === b.id}
          onclick={() => equip({ banner: b.id })}
        >
          <span class="banner-chip banner-{b.id}"></span>
          {b.name}
          {#if locked}<span class="lock" aria-label={unlocksWith(`banner:${b.id}`)}>🔒</span>{/if}
        </button>
      {/each}
    </div>
  </section>
</section>

{#if cropping}
  <AvatarCropDialog
    src={cropping}
    onclose={() => (cropping = null)}
    onsaved={(a) => {
      cropping = null;
      onaccount(a);
      toast("Picture saved", "success");
    }}
  />
{/if}

<style>
  .hero {
    display: flex;
    align-items: center;
    gap: 1.25rem;
    padding: 1rem 0;
  }
  .hero.has-banner {
    padding: 4rem 1.5rem 1.25rem;
    border-radius: var(--radius-lg);
    background: linear-gradient(to top, rgba(0, 0, 0, 0.55), transparent 70%), var(--banner);
    box-shadow: inset 0 0 0 1px var(--border);
  }
  h1 {
    margin: 0;
  }
  .handle,
  .count {
    margin: 0.2rem 0 0;
    color: var(--text-muted);
  }
  .group h2 {
    margin: 0 0 0.75rem;
    font-size: 1.05rem;
  }
  .group h3 {
    margin: 1.25rem 0 0.6rem;
    font-size: 0.9rem;
    color: var(--text-muted);
  }
  .muted {
    color: var(--text-muted);
    margin: 0.5rem 0 0;
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
    display: flex;
    gap: 0.75rem;
    align-items: center;
    padding: 0.7rem 0.85rem;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--border);
    opacity: 0.55;
  }
  .badge.earned {
    opacity: 1;
    background: linear-gradient(135deg, var(--accent-softer), var(--surface) 60%);
    box-shadow: inset 0 0 0 1px var(--accent-soft);
  }
  .badge:not(.earned) .medal {
    filter: grayscale(1);
  }
  .medal {
    display: grid;
    place-items: center;
    flex: none;
    width: 2.6rem;
    height: 2.6rem;
    border-radius: 50%;
    background: var(--surface-2);
    font-size: 1.35rem;
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
  .row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .choice {
    position: relative;
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    gap: 0.4rem;
    min-width: 5rem;
    padding: 0.6rem 0.6rem 0.5rem;
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--border);
    color: var(--text-muted);
    font: inherit;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .choice.on {
    box-shadow: inset 0 0 0 2px var(--accent);
    color: var(--text);
  }
  .choice:disabled:not(.on) {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .lock {
    position: absolute;
    top: 0.25rem;
    right: 0.35rem;
    font-size: 0.7rem;
  }
  .swatch {
    width: 1.75rem;
    height: 1.75rem;
    border-radius: 50%;
    background: var(--c);
    box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.15);
  }
  .theme-chip {
    width: 2.5rem;
    height: 1.75rem;
    border-radius: 7px;
    background: linear-gradient(135deg, var(--b) 55%, var(--s) 55%);
    box-shadow: inset 0 0 0 1px var(--s);
  }
  .banner-chip {
    width: 5.5rem;
    height: 1.75rem;
    border-radius: 7px;
    background: var(--banner);
  }
</style>
