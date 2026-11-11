<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorCode, errorMessage, type FeedClip, type GameCount } from "../lib/api";
  import { formatDuration, groupByDay, hueOf, relativeDate } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import Avatar from "../components/Avatar.svelte";
  import GameFilter from "../components/GameFilter.svelte";
  import Icon from "../components/Icon.svelte";

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
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let loaded = $state(false);
  let loadError = $state<string | null>(null);
  let playing = $state<FeedClip | null>(null);
  let now = $state(Date.now());

  const groups = $derived(groupByDay(clips, (c) => new Date(c.createdAt), now));

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
      const page = await api.feedClips(game, more ? cursor : null);
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

  async function copyLink(clip: FeedClip) {
    try {
      await api.copyText(clip.shareUrl);
      toast("Link copied!", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  function openOnWeb(clip: FeedClip) {
    api.openWebsite(clip.shareUrl).catch((e) => toast(errorMessage(e), "error"));
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && playing) playing = null;
  }

  onMount(refresh);
</script>

<svelte:window {onkeydown} />

<section class="page">
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

  {#if games.length > 0}
    <GameFilter {games} selected={game} onselect={selectGame} />
  {/if}

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
      {#if game}
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
                  <span class="play"><Icon name="play" size={22} /></span>
                  {#if clip.gameName}
                    <span class="game-tag" style:--hue={hueOf(clip.gameName)}>
                      <span class="game-dot"></span>{clip.gameName}
                    </span>
                  {/if}
                  <span class="dur">{formatDuration(clip.durationMs)}</span>
                </button>
                <div class="body">
                  <Avatar name={clip.owner.username} size={32} />
                  <div class="text">
                    <div class="title" title={clip.title}>{clip.title}</div>
                    <div class="meta">
                      <span class="who">{clip.owner.username}</span>
                      <span aria-hidden="true">·</span>
                      <span>{relativeDate(new Date(clip.createdAt), now)}</span>
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
  <div class="backdrop" onclick={(e) => e.target === e.currentTarget && (playing = null)} role="presentation">
    <div class="player" role="dialog" aria-modal="true" aria-label={playing.title}>
      <div class="player-head">
        <Avatar name={playing.owner.username} size={38} />
        <div class="text">
          <div class="title">{playing.title}</div>
          <div class="meta">
            <span class="who">{playing.owner.username}</span>
            {#if playing.gameName}
              <span aria-hidden="true">·</span>
              <span class="game"><span class="game-dot" style:--hue={hueOf(playing.gameName)}></span>{playing.gameName}</span>
            {/if}
            <span aria-hidden="true">·</span>
            <span>{relativeDate(new Date(playing.createdAt), now)}</span>
          </div>
        </div>
        <div class="tools">
          <button class="btn btn-sm" onclick={() => playing && copyLink(playing)}>
            <Icon name="link" size={15} /> Copy link
          </button>
          <button class="btn btn-sm btn-ghost" onclick={() => playing && openOnWeb(playing)}>
            <Icon name="external" size={15} /> Open on web
          </button>
          <button class="btn btn-sm btn-ghost btn-icon" aria-label="Close" onclick={() => (playing = null)}>
            <Icon name="close" />
          </button>
        </div>
      </div>
      <!-- svelte-ignore a11y_media_has_caption -->
      <video src={playing.videoUrl} poster={playing.thumbnailUrl} controls autoplay></video>
    </div>
  </div>
{/if}

<style>
  .refresh-icon {
    display: inline-grid;
  }

  .spinning {
    animation: spin 0.9s linear infinite;
  }

  .card {
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
    max-width: calc(100% - 1.1rem);
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

  .game {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
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

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 2rem;
    background: rgba(5, 4, 10, 0.82);
    backdrop-filter: blur(6px);
    animation: fade 0.18s ease-out;
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }

  .player {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    width: min(1100px, 100%);
    padding: 0.9rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow);
    animation: pop 0.22s var(--ease-out);
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }

  .player-head {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    min-width: 0;
    padding: 0 0.15rem;
  }

  .player-head .title {
    font-family: var(--font-display);
    font-size: 1.1rem;
    font-weight: 700;
  }

  .tools {
    display: flex;
    flex: none;
    align-items: center;
    gap: 0.35rem;
  }

  video {
    width: 100%;
    max-height: calc(100vh - 12rem);
    background: #000;
    border-radius: var(--radius);
  }
</style>
