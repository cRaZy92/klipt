<script lang="ts">
  import { untrack } from "svelte";
  import { ACCENTS, ACHIEVEMENTS, BANNERS, FRAMES, THEMES, type CosmeticId, type Looks, type MyAchievements } from "@klipt/shared";
  import { api, errorCode, errorMessage, type Account } from "../lib/api";
  import { pickProfilePicture } from "../lib/thumbnailImage";
  import { toast } from "../lib/toasts.svelte";
  import Avatar from "../components/Avatar.svelte";
  import AvatarCropDialog from "../components/AvatarCropDialog.svelte";
  import Icon from "../components/Icon.svelte";
  import AchievementCard from "../components/AchievementCard.svelte";
  import { AchievementTips } from "../lib/achievementTip.svelte";

  // How you look: your picture, and the colours, themes, frames and banners you can pick (locked
  // ones say which achievement unlocks them). The card at the top is how friends see you.
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

  // Again each time the screen comes back: the looks are unlocked by achievements earned in the background.
  $effect(() => {
    if (active) untrack(() => void load());
  });

  const unlocked = $derived(new Set<string>(mine?.unlocked ?? []));
  const looks = $derived<Looks>(mine?.looks ?? account.looks ?? { accent: "klipt", theme: "klipt", frame: null, banner: null });

  const tips = new AchievementTips();
  $effect(() => {
    if (!active) tips.hide();
  });

  /** The achievement that unlocks a look, for its card. */
  function unlocker(id: CosmeticId) {
    const a = ACHIEVEMENTS.find((x) => x.reward === id);
    return a ? { achievement: a } : null;
  }

  function unlocksWith(id: CosmeticId): string {
    const a = ACHIEVEMENTS.find((x) => x.reward === id);
    if (!a) return "Locked";
    return a.hidden ? "Unlocks with a secret achievement" : `Unlocks with ${a.name}: ${a.description}`;
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

{#snippet lock(id: CosmeticId)}
  <span class="lock" aria-label={unlocksWith(id)}><Icon name="lock" size={12} /></span>
{/snippet}

<section class="page appearance">
  <header>
    <h1>Appearance</h1>
    <p class="muted">Colours and themes change what you see. Your picture, frame and banner are what friends see on you.</p>
  </header>

  <div class="hero banner-{looks.banner ?? 'none'}" class:has-banner={!!looks.banner}>
    <Avatar name={account.displayName} url={account.avatarUrl} frame={looks.frame} size={88} />
    <div>
      <strong class="name">{account.displayName}</strong>
      {#if account.displayName !== account.username}<p class="handle">@{account.username}</p>{/if}
    </div>
  </div>

  {#if loadError}
    <p class="error" role="alert">{loadError} <button class="btn btn-sm" onclick={load}>Try again</button></p>
  {/if}

  <section class="group">
    <h3>Picture</h3>
    <div class="row">
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
          class:locked
          disabled={busy && !locked}
          aria-disabled={locked || undefined}
          title={locked ? undefined : a.name}
          aria-pressed={looks.accent === a.id}
          use:tips.trigger={locked ? unlocker(`accent:${a.id}`) : null}
          onclick={() => !locked && equip({ accent: a.id })}
        >
          <span class="swatch" style:--c={a.color}></span>
          {a.name}
          {#if locked}{@render lock(`accent:${a.id}`)}{/if}
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
          class:locked
          disabled={busy && !locked}
          aria-disabled={locked || undefined}
          title={locked ? undefined : t.name}
          aria-pressed={looks.theme === t.id}
          use:tips.trigger={locked ? unlocker(`theme:${t.id}`) : null}
          onclick={() => !locked && equip({ theme: t.id })}
        >
          <span class="theme-chip" style:--b={t.tokens["--bg"]} style:--s={t.tokens["--surface-3"]}></span>
          {t.name}
          {#if locked}{@render lock(`theme:${t.id}`)}{/if}
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
          class:locked
          disabled={busy && !locked}
          aria-disabled={locked || undefined}
          title={locked ? undefined : f.name}
          aria-pressed={looks.frame === f.id}
          use:tips.trigger={locked ? unlocker(`frame:${f.id}`) : null}
          onclick={() => !locked && equip({ frame: f.id })}
        >
          <Avatar name={account.displayName} url={account.avatarUrl} frame={f.id} size={32} />
          {f.name}
          {#if locked}{@render lock(`frame:${f.id}`)}{/if}
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
          class:locked
          disabled={busy && !locked}
          aria-disabled={locked || undefined}
          title={locked ? undefined : b.name}
          aria-pressed={looks.banner === b.id}
          use:tips.trigger={locked ? unlocker(`banner:${b.id}`) : null}
          onclick={() => !locked && equip({ banner: b.id })}
        >
          <span class="banner-chip banner-{b.id}"></span>
          {b.name}
          {#if locked}{@render lock(`banner:${b.id}`)}{/if}
        </button>
      {/each}
    </div>
  </section>
</section>

<AchievementCard tip={tips.current} name={account.displayName} avatarUrl={account.avatarUrl} onclose={tips.hide} />

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
  header h1 {
    margin: 0 0 4px;
  }
  .muted {
    color: var(--text-muted);
    margin: 0;
  }
  /* How friends see you. */
  .hero {
    display: flex;
    align-items: center;
    gap: 1.25rem;
    padding: 1.25rem 1.5rem;
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--border);
  }
  .hero.has-banner {
    padding-top: 4rem;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.55), transparent 70%), var(--banner);
  }
  .name {
    font-size: 1.4rem;
  }
  .handle {
    margin: 0.2rem 0 0;
    color: var(--text-muted);
  }
  .group h3 {
    margin: 1.25rem 0 0.6rem;
    font-size: 0.9rem;
    color: var(--text-muted);
  }
  .group h3:first-child {
    margin-top: 0;
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
  .choice.locked {
    cursor: default;
    opacity: 0.55;
  }
  .choice.locked:hover,
  .choice.locked:focus-visible {
    opacity: 0.85;
  }
  .lock {
    position: absolute;
    top: 0.3rem;
    right: 0.35rem;
    display: grid;
    color: var(--text-faint);
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
