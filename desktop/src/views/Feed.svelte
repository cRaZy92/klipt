<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, errorCode, errorMessage, type FeedClip, type GameCount } from "../lib/api";
  import { neighbors } from "../lib/clipNav";
  import { formatDuration, groupByDay, hueOf, relativeDate, viewsLabel } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import Avatar from "../components/Avatar.svelte";
  import ClipPlayer from "../components/ClipPlayer.svelte";
  import GameFilter from "../components/GameFilter.svelte";
  import Icon from "../components/Icon.svelte";
  import ProcessingPill from "../components/ProcessingPill.svelte";
  import SearchInput from "../components/SearchInput.svelte";

  let {
    server = null,
    onunauthorized,
  }: {
    /** The server's host name, shown under the heading. */
    server?: string | null;
    onunauthorized: () => void;
  } = $props();

  let clips = $state<FeedClip[]>([]);
  let games = $state<GameCount[]>([]);
  let game = $state<string | null>(null);
  let search = $state("");
  /** Only clips you're tagged in. */
  let withMe = $state(false);
  /** Only video clips or only screenshots. */
  let kind = $state<"clip" | "screenshot" | null>(null);
  /** Only clips you starred. */
  let favoritesOnly = $state(false);
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let loaded = $state(false);
  let loadError = $state<string | null>(null);
  let playing = $state<FeedClip | null>(null);
  let now = $state(Date.now());

  const groups = $derived(groupByDay(clips, (c) => new Date(c.createdAt), now));
  const around = $derived(playing ? neighbors(clips, playing.id) : { prev: null, next: null });

  // Playing the last loaded clip fetches older ones, so "Up next" doesn't stop at a page break.
  // Once per page: a failed fetch isn't retried in a loop.
  let autoLoaded: string | null = null;
  $effect(() => {
    if (!playing || around.next || !cursor || loading || cursor === autoLoaded) return;
    autoLoaded = cursor;
    untrack(() => void load(true));
  });

  // Bumped so answers to older requests are dropped when the filter changes quickly.
  let request = 0;

  function failed(e: unknown): string {
    if (errorCode(e) === "unauthorized") onunauthorized();
    return errorMessage(e);
  }

  async function load(more = false) {
    const id = ++request;
    loading = true;
    if (!more) loadError = null;
    try {
      const page = await api.feedClips(game, more ? cursor : null, search.trim() || null, withMe, kind, favoritesOnly);
      if (id !== request) return;
      clips = more ? [...clips, ...page.items] : page.items;
      cursor = page.nextCursor;
      // The unfiltered first page is the whole feed's newest end: you've now seen it.
      if (!more && !game && !search.trim() && !withMe && !kind && !favoritesOnly && page.items[0]) api.markFeedSeen(page.items[0].createdAt).catch(() => {});
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
      games = (await api.feedGames()).map((g) => ({ name: g.gameName, steamAppId: g.steamAppId, count: g.count }));
    } catch (e) {
      failed(e);
    }
  }

  function refresh() {
    void loadGames();
    void load();
  }

  function selectGame(g: string | null) {
    game = g;
    void load();
  }

  /** Stars or unstars a clip: shown at once, put back if the server refuses. */
  async function toggleFavorite(clip: FeedClip) {
    const favorite = !clip.favorite;
    const set = (on: boolean) => (clips = clips.map((c) => (c.id === clip.id ? { ...c, favorite: on } : c)));
    set(favorite);
    try {
      await api.setServerFavorite(clip.id, favorite);
      // Unstarred while viewing favorites: it no longer belongs here.
      if (favoritesOnly && !favorite) clips = clips.filter((c) => c.id !== clip.id);
    } catch (e) {
      set(!favorite);
      toast(failed(e), "error");
    }
  }

  async function copyLink(clip: FeedClip) {
    try {
      await api.copyText(clip.shareUrl);
      toast("Link copied!", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  onMount(refresh);
</script>

<section class="page">
  <div class="page-top">
    <header class="page-head">
      <div>
        <h1>Feed</h1>
        <p class="subtitle">Clips your friends shared{server ? ` on ${server}` : ""}.</p>
      </div>
      <div class="page-tools">
        <button class="btn btn-ghost" onclick={() => api.openWebsite().catch((e) => toast(errorMessage(e), "error"))}>
          <Icon name="external" size={16} /> Open website
        </button>
        <button class="btn" onclick={refresh} disabled={loading}>
          <span class="refresh-icon" class:spinning={loading}><Icon name="retry" size={16} /></span> Refresh
        </button>
      </div>
    </header>

    <div class="filters">
      <SearchInput
        bind:value={search}
        placeholder="Search by title, game or friend"
        label="Search the feed"
        delay={300}
        onsearch={() => void load()}
      />
      <button
        class="with-me"
        class:on={withMe}
        aria-pressed={withMe}
        title="Clips your friends tagged you in"
        onclick={() => {
          withMe = !withMe;
          void load();
        }}
      >
        I'm in it
      </button>
      <button
        class="with-me"
        class:on={favoritesOnly}
        aria-pressed={favoritesOnly}
        title="Clips you starred"
        onclick={() => {
          favoritesOnly = !favoritesOnly;
          void load();
        }}
      >
        <Icon name={favoritesOnly ? "starFilled" : "star"} size={14} /> Favorites
      </button>
      {#each [["clip", "Clips"], ["screenshot", "Screenshots"]] as const as [value, label] (value)}
        <button
          class="with-me"
          class:on={kind === value}
          aria-pressed={kind === value}
          onclick={() => {
            kind = kind === value ? null : value;
            void load();
          }}
        >
          {label}
        </button>
      {/each}
    </div>

    {#if games.length > 0}
      <GameFilter {games} selected={game} onselect={selectGame} />
    {/if}
  </div>

  {#if loadError}
    <div class="empty">
      <div class="empty-icon error"><Icon name="alert" size={28} /></div>
      <p>{loadError}</p>
      <button class="btn" onclick={refresh}><Icon name="retry" size={16} /> Try again</button>
    </div>
  {:else if !loaded}
    <div class="clip-grid" aria-busy="true" aria-label="Loading the feed">
      {#each { length: 8 }, i (i)}
        <div class="card ghost">
          <div class="skeleton thumb-ghost"></div>
          <div class="body">
            <div class="skeleton dot"></div>
            <div class="lines">
              <div class="skeleton line"></div>
              <div class="skeleton line short"></div>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {:else if clips.length === 0}
    <div class="empty">
      <div class="empty-icon"><Icon name="users" size={28} /></div>
      {#if search.trim()}
        <p>No clips match “{search.trim()}”.</p>
      {:else if favoritesOnly}
        <p>No favorites yet. Star a clip to keep it here.</p>
      {:else if withMe}
        <p>Nobody has tagged you in a clip yet.</p>
      {:else if game}
        <p>No clips for this game yet.</p>
      {:else}
        <h2>Nothing shared yet</h2>
        <p>Upload a clip from My clips and it shows up here for everyone.</p>
      {/if}
    </div>
  {:else}
    <div class="clip-groups">
      {#each groups as group (group.key)}
        <section aria-label={group.label}>
          <h2 class="group-label">{group.label}</h2>
          <div class="clip-grid">
            {#each group.items as clip, i (clip.id)}
              <article class="card rise" style:--i={i}>
                <button class="thumb" onclick={() => (playing = clip)} aria-label={`Play ${clip.title}`}>
                  <img src={clip.thumbnailUrl} alt="" loading="lazy" />
                  <span class="play"><Icon name={clip.kind === "screenshot" ? "image" : "play"} size={22} /></span>
                  {#if clip.gameName}
                    <span class="game-tag" style:--hue={hueOf(clip.gameName)}>
                      <span class="game-dot"></span>{clip.gameName}
                    </span>
                  {/if}
                  <ProcessingPill state={clip.processing} />
                  <span class="dur">{clip.kind === "screenshot" ? "Screenshot" : formatDuration(clip.durationMs)}</span>
                </button>
                {#if clip.favorite !== undefined}
                  <button
                    class="star"
                    class:on={clip.favorite}
                    onclick={() => toggleFavorite(clip)}
                    aria-pressed={clip.favorite}
                    aria-label={clip.favorite ? "Remove from favorites" : "Add to favorites"}
                    title={clip.favorite ? "In your favorites" : "Add to favorites"}
                  >
                    <Icon name={clip.favorite ? "starFilled" : "star"} size={16} />
                  </button>
                {/if}
                <div class="body">
                  <Avatar name={clip.owner.username} size={32} />
                  <div class="text">
                    <div class="title" title={clip.title}>{clip.title}</div>
                    <div class="meta">
                      <span class="who">{clip.owner.username}</span>
                      <span aria-hidden="true">·</span>
                      <span>{relativeDate(new Date(clip.createdAt), now)}</span>
                      {#if clip.views > 0}
                        <span aria-hidden="true">·</span>
                        <span>{viewsLabel(clip.views)}</span>
                      {/if}
                    </div>
                  </div>
                </div>
              </article>
            {/each}
          </div>
        </section>
      {/each}
    </div>
    {#if cursor}
      <div class="more">
        <button class="btn" onclick={() => load(true)} disabled={loading}>
          {#if loading}<span class="spinner"></span>{/if} Load older clips
        </button>
      </div>
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
  />
{/if}

<style>
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
  }

  .with-me {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.42rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-muted);
    font: inherit;
    font-size: 0.88rem;
    font-weight: 600;
    cursor: pointer;
  }

  .with-me:hover {
    color: var(--text);
  }

  /* Favorite star: always shown when starred, on hover or focus otherwise (like My clips). */
  .star {
    position: absolute;
    top: 0.45rem;
    right: 0.45rem;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 2rem;
    height: 2rem;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 50%;
    background: rgba(12, 10, 20, 0.7);
    backdrop-filter: blur(8px);
    color: #fff;
    cursor: pointer;
    opacity: 0;
    transition:
      opacity 0.15s,
      color 0.15s;
  }

  .card:hover .star,
  .star:focus-visible,
  .star.on {
    opacity: 1;
  }

  .star.on {
    color: #ffc53d;
  }

  .with-me.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }

  .refresh-icon {
    display: inline-grid;
  }

  .spinning {
    animation: spin 0.9s linear infinite;
  }

  .card {
    position: relative;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    transition:
      border-color 0.2s,
      box-shadow 0.2s,
      transform 0.2s var(--ease-out);
  }

  .card:not(.ghost):hover {
    border-color: var(--border-strong);
    box-shadow: 0 16px 36px -18px rgba(0, 0, 0, 0.8);
    transform: translateY(-2px);
  }

  .thumb {
    position: relative;
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    padding: 0;
    border: 0;
    background: #000;
    cursor: pointer;
  }

  .thumb::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(rgba(5, 4, 10, 0.4), transparent 35%, transparent 60%, rgba(5, 4, 10, 0.5));
    pointer-events: none;
  }

  .thumb:focus-visible {
    outline-offset: -3px;
  }

  .thumb img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    transition: transform 0.45s var(--ease-out);
  }

  .card:hover .thumb img {
    transform: scale(1.05);
  }

  .play {
    position: absolute;
    top: 50%;
    left: 50%;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 3.25rem;
    height: 3.25rem;
    padding-left: 3px;
    border-radius: 50%;
    background: rgba(124, 92, 255, 0.9);
    box-shadow: 0 8px 26px -4px var(--accent-glow);
    color: #fff;
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.85);
    transition:
      opacity 0.2s,
      transform 0.2s var(--ease-out);
  }

  .thumb:hover .play,
  .thumb:focus-visible .play {
    opacity: 1;
    transform: translate(-50%, -50%);
  }

  .game-tag {
    position: absolute;
    top: 0.55rem;
    left: 0.55rem;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    max-width: calc(100% - 3.6rem);
    overflow: hidden;
    padding: 0.2rem 0.55rem;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 999px;
    background: rgba(12, 10, 20, 0.7);
    backdrop-filter: blur(8px);
    color: #fff;
    font-size: 0.74rem;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dur {
    position: absolute;
    right: 0.55rem;
    bottom: 0.5rem;
    z-index: 1;
    padding: 0.1rem 0.45rem;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    font-size: 0.78rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }

  .body {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    padding: 0.75rem 0.9rem 0.85rem;
  }

  .text {
    flex: 1;
    min-width: 0;
  }

  .title {
    overflow: hidden;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.1rem 0.4rem;
    margin-top: 0.1rem;
    color: var(--text-muted);
    font-size: 0.84rem;
  }

  .who {
    color: var(--text);
    font-weight: 600;
  }

  /* Loading placeholders */
  .thumb-ghost {
    aspect-ratio: 16 / 9;
    border-radius: 0;
  }

  .dot {
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 50%;
  }

  .lines {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 0.45rem;
  }

  .line {
    height: 0.75rem;
    border-radius: 4px;
  }

  .line.short {
    width: 55%;
  }

  .more {
    display: flex;
    justify-content: center;
  }
</style>
