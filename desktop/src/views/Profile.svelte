<script lang="ts">
  import { untrack } from "svelte";
  import { joinedLabel } from "@klipt/shared";
  import { api, errorCode, errorMessage, type FeedClip, type GameCount, type UserProfile } from "../lib/api";
  import { neighbors } from "../lib/clipNav";
  import { groupByDay, viewsLabel } from "../lib/format";
  import { forgetProfiles, loadProfile } from "../lib/profiles.svelte";
  import { toast } from "../lib/toasts.svelte";
  import Avatar from "../components/Avatar.svelte";
  import ClipPlayer from "../components/ClipPlayer.svelte";
  import FeedCard from "../components/FeedCard.svelte";
  import GameFilter from "../components/GameFilter.svelte";
  import Icon from "../components/Icon.svelte";
  import SearchInput from "../components/SearchInput.svelte";

  // Someone's profile: their banner, picture, badges and totals, and their clips as far as you
  // may see them (all for a friend or yourself, only public ones otherwise). Built per person.
  let {
    username,
    active = true,
    onback,
    onunauthorized,
  }: {
    username: string;
    active?: boolean;
    /** Back to the screen the profile was opened from. */
    onback: () => void;
    onunauthorized: () => void;
  } = $props();

  let profile = $state<UserProfile | null>(null);
  let profileError = $state<string | null>(null);
  let clips = $state<FeedClip[]>([]);
  let games = $state<GameCount[]>([]);
  let game = $state<string | null>(null);
  let search = $state("");
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let loaded = $state(false);
  let loadError = $state<string | null>(null);
  let playing = $state<FeedClip | null>(null);
  let busy = $state(false);
  let now = $state(Date.now());

  const groups = $derived(groupByDay(clips, (c) => new Date(c.createdAt), now));
  const around = $derived(playing ? neighbors(clips, playing.id) : { prev: null, next: null });
  const playingNow = $derived(playing ? (clips.find((c) => c.id === playing!.id) ?? playing) : null);
  const name = $derived(profile?.user.displayName ?? username);

  function failed(e: unknown): string {
    if (errorCode(e) === "unauthorized") onunauthorized();
    return errorMessage(e);
  }

  async function loadHead(fresh = false) {
    profileError = null;
    try {
      profile = await loadProfile(username, fresh);
    } catch (e) {
      profileError = failed(e);
    }
  }

  let request = 0;
  async function load(more = false) {
    const id = ++request;
    loading = true;
    if (!more) loadError = null;
    try {
      const page = await api.feedClips(game, more ? cursor : null, search.trim() || null, false, null, false, username);
      if (id !== request) return;
      clips = more ? [...clips, ...page.items] : page.items;
      cursor = page.nextCursor;
      now = Date.now();
      loaded = true;
    } catch (e) {
      if (id !== request) return;
      const message = failed(e);
      if (more) toast(message, "error");
      else loadError = message;
    } finally {
      if (id === request) loading = false;
    }
  }

  async function loadGames() {
    try {
      games = (await api.feedGames(username)).map((g) => ({ name: g.gameName, steamAppId: g.steamAppId, count: g.count }));
    } catch (e) {
      failed(e);
    }
  }

  $effect(() => {
    void username;
    untrack(() => {
      void loadHead();
      void loadGames();
      void load();
    });
  });

  // Playing the last loaded clip fetches older ones, like the Feed.
  let autoLoaded: string | null = null;
  $effect(() => {
    if (!playing || around.next || !cursor || loading || cursor === autoLoaded) return;
    autoLoaded = cursor;
    untrack(() => void load(true));
  });

  let nearEnd = $state(false);
  $effect(() => {
    if (!active || !nearEnd || !cursor || loading || playing) return;
    untrack(() => void load(true));
  });
  function watchEnd(node: HTMLElement) {
    const observer = new IntersectionObserver(([entry]) => (nearEnd = entry.isIntersecting), {
      root: node.closest(".screen"),
      rootMargin: "0px 0px 150% 0px",
    });
    observer.observe(node);
    return {
      destroy() {
        observer.disconnect();
        nearEnd = false;
      },
    };
  }

  $effect(() => {
    if (!active) playing = null;
  });

  async function friend(add: boolean) {
    if (!profile) return;
    busy = true;
    try {
      const before = profile.friendState;
      if (add) await api.addFriend(username);
      else await api.removeFriend(username);
      forgetProfiles();
      await loadHead(true);
      const after = profile?.friendState;
      toast(
        after === "friends" ? `You and ${name} are friends now`
        : after === "outgoing" ? "Friend request sent"
        : before === "friends" ? `${name} is no longer your friend`
        : before === "incoming" ? "Request declined"
        : "Request cancelled",
        "success",
      );
      // Becoming (or no longer being) friends changes which clips you may see.
      void loadGames();
      void load();
    } catch (e) {
      toast(failed(e), "error");
    } finally {
      busy = false;
    }
  }

  async function toggleFavorite(clip: FeedClip) {
    const favorite = !clip.favorite;
    const set = (on: boolean) => (clips = clips.map((c) => (c.id === clip.id ? { ...c, favorite: on } : c)));
    set(favorite);
    if (playing?.id === clip.id) playing = { ...playing, favorite };
    try {
      await api.setServerFavorite(clip.id, favorite);
    } catch (e) {
      set(!favorite);
      if (playing?.id === clip.id) playing = { ...playing, favorite: !favorite };
      toast(failed(e), "error");
    }
  }

  function openWebsite() {
    if (profile) api.openWebsite(profile.profileUrl).catch((e) => toast(errorMessage(e), "error"));
  }

  function onkeydown(e: KeyboardEvent) {
    if (!active || playing || e.key !== "Escape") return;
    if ((e.target as HTMLElement | null)?.closest("input, textarea, select, [contenteditable]")) return;
    onback();
  }
</script>

<svelte:window {onkeydown} />

<section class="page">
  <div class="top">
    <button class="btn btn-ghost btn-sm" onclick={onback}><Icon name="back" size={16} /> Back</button>
    {#if profile}
      <button class="btn btn-ghost btn-sm" onclick={openWebsite}><Icon name="external" size={16} /> Open on website</button>
    {/if}
  </div>

  {#if profileError}
    <div class="empty">
      <div class="empty-icon error"><Icon name="alert" size={28} /></div>
      <p>{profileError}</p>
      <button class="btn" onclick={() => loadHead(true)}><Icon name="retry" size={16} /> Try again</button>
    </div>
  {:else}
    <header class="hero banner-{profile?.banner ?? 'none'}" class:has-banner={!!profile?.banner}>
      <Avatar name={name} url={profile?.user.avatarUrl ?? null} frame={profile?.user.frame ?? null} size={88} />
      <div class="names">
        <h1>{name}</h1>
        <p class="handle">
          @{profile?.user.username ?? username}{#if profile?.joinedAt}<span class="joined">{" · "}{joinedLabel(profile.joinedAt)}</span>{/if}
        </p>
      </div>
      {#if profile && profile.friendState !== "self"}
        <div class="actions">
          {#if profile.friendState === "none"}
            <button class="btn btn-primary" disabled={busy} onclick={() => friend(true)}>Add friend</button>
          {:else if profile.friendState === "outgoing"}
            <button class="btn" disabled={busy} onclick={() => friend(false)}>Requested · Cancel</button>
          {:else if profile.friendState === "incoming"}
            <button class="btn btn-primary" disabled={busy} onclick={() => friend(true)}>Accept</button>
            <button class="btn" disabled={busy} onclick={() => friend(false)}>Decline</button>
          {:else if profile.friendState === "friends"}
            <span class="friends-pill">Friends</span>
            <button class="btn btn-sm btn-ghost" disabled={busy} onclick={() => friend(false)}>Unfriend</button>
          {/if}
        </div>
      {/if}
    </header>

    {#if profile && !profile.full}
      <p class="muted private">Only friends see everything {name} posts. These are their public clips.</p>
    {/if}

    {#if profile && profile.badges.length > 0}
      <ul class="badges" aria-label="Achievements">
        {#each profile.badges as b (b.id)}
          <li title={b.description}><span aria-hidden="true">{b.icon}</span>{b.name}</li>
        {/each}
      </ul>
    {/if}

    {#if profile?.stats && profile.stats.clips > 0}
      <div class="stats">
        <div><b>{profile.stats.clips}</b><span>{profile.stats.clips === 1 ? "clip" : "clips"}</span></div>
        <div><b>{profile.stats.views}</b><span>{profile.stats.views === 1 ? "view" : "views"}</span></div>
        {#if profile.stats.top}
          {@const top = profile.stats.top}
          {@const topClip = clips.find((c) => c.id === top.id)}
          <div class="top-clip">
            {#if topClip}
              <button class="link" onclick={() => (playing = topClip)}><b>{top.title}</b></button>
            {:else}
              <b>{top.title}</b>
            {/if}
            <span>most watched · {viewsLabel(top.views)}</span>
          </div>
        {/if}
      </div>
    {/if}

    <div class="filters">
      <SearchInput bind:value={search} placeholder="Search {name}'s clips" label="Search these clips" delay={300} onsearch={() => void load()} />
    </div>
    {#if games.length > 0}
      <GameFilter
        {games}
        selected={game}
        onselect={(g) => {
          game = g;
          void load();
        }}
      />
    {/if}

    {#if loadError}
      <div class="empty">
        <div class="empty-icon error"><Icon name="alert" size={28} /></div>
        <p>{loadError}</p>
        <button class="btn" onclick={() => load()}><Icon name="retry" size={16} /> Try again</button>
      </div>
    {:else if !loaded}
      <div class="clip-grid" aria-busy="true" aria-label="Loading clips">
        {#each { length: 4 }, i (i)}
          <FeedCard clip={null} />
        {/each}
      </div>
    {:else if clips.length === 0}
      <div class="empty">
        <div class="empty-icon"><Icon name="users" size={28} /></div>
        {#if search.trim()}
          <p>No clips match “{search.trim()}”.</p>
        {:else if game}
          <p>No clips for this game.</p>
        {:else}
          <p>No clips here yet.</p>
        {/if}
      </div>
    {:else}
      <div class="clip-groups">
        {#each groups as group (group.key)}
          <section aria-label={group.label}>
            <h2 class="group-label">{group.label}</h2>
            <div class="clip-grid">
              {#each group.items as clip, i (clip.id)}
                <FeedCard {clip} {now} index={i} ownerLink={false} onplay={() => (playing = clip)} onfavorite={() => toggleFavorite(clip)} />
              {/each}
            </div>
          </section>
        {/each}
      </div>
      {#if cursor}
        <div class="more" use:watchEnd>
          <span class="spinner" role="status" aria-label="Loading older clips"></span>
        </div>
      {/if}
    {/if}
  {/if}
</section>

{#if playing}
  <ClipPlayer
    clip={playing}
    {now}
    prev={around.prev}
    next={around.next}
    onnavigate={(c) => (playing = c)}
    onclose={() => (playing = null)}
    favorite={!!playingNow?.favorite}
    onfavorite={playingNow && playingNow.favorite !== undefined ? () => toggleFavorite(playingNow) : undefined}
  />
{/if}

<style>
  .top {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
  }
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
    padding-top: 4.5rem;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.55), transparent 70%), var(--banner);
  }
  .names {
    min-width: 0;
  }
  .names h1 {
    margin: 0;
    overflow: hidden;
    font-family: var(--font-display);
    font-size: 1.75rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .handle {
    margin: 0.2rem 0 0;
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-left: auto;
  }
  .friends-pill {
    padding: 0.25rem 0.75rem;
    border-radius: 999px;
    background: var(--success-soft);
    color: var(--success);
    font-size: 0.85rem;
    font-weight: 650;
  }
  .private {
    margin: 1rem 0 0;
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin: 1rem 0 0;
    padding: 0;
    list-style: none;
  }
  .badges li {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.65rem 0.25rem 0.4rem;
    border-radius: 999px;
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--border);
    color: var(--text-muted);
    font-size: 0.85rem;
    font-weight: 600;
    cursor: default;
  }
  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    margin-top: 1rem;
  }
  .stats > div {
    display: flex;
    flex-direction: column;
    min-width: 6rem;
    padding: 0.65rem 1rem;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--border);
  }
  .stats b {
    font-family: var(--font-display);
    font-size: 1.25rem;
  }
  .stats span {
    color: var(--text-muted);
    font-size: 0.82rem;
  }
  .stats .top-clip {
    flex: 1;
    min-width: 12rem;
    max-width: 24rem;
  }
  .top-clip b {
    display: block;
    overflow: hidden;
    font-size: 1rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link {
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .link:hover b {
    color: var(--accent-text);
  }
  .filters {
    display: flex;
    margin: 1.25rem 0 0.75rem;
  }
  .more {
    display: flex;
    justify-content: center;
    min-height: 2.5rem;
    padding-top: 1.5rem;
  }
</style>
