<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorCode, errorMessage, type FeedClip, type GameCount } from "../lib/api";
  import { formatDuration, relativeDate } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import GameFilter from "../components/GameFilter.svelte";
  import Icon from "../components/Icon.svelte";

  let { onunauthorized }: { onunauthorized: () => void } = $props();

  let clips = $state<FeedClip[]>([]);
  let games = $state<GameCount[]>([]);
  let game = $state<string | null>(null);
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let loaded = $state(false);
  let loadError = $state<string | null>(null);
  let playing = $state<FeedClip | null>(null);
  let now = $state(Date.now());

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
    api.openWebsite(new URL(clip.shareUrl).pathname).catch((e) => toast(errorMessage(e), "error"));
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && playing) playing = null;
  }

  onMount(refresh);
</script>

<svelte:window {onkeydown} />

<section class="feed">
  <div class="toolbar">
    <h1>Feed</h1>
    <div class="tools">
      <button class="btn btn-ghost" onclick={() => api.openWebsite().catch((e) => toast(errorMessage(e), "error"))}>
        <Icon name="link" size={16} /> Open website
      </button>
      <button class="btn" onclick={refresh} disabled={loading}>
        <Icon name="retry" size={16} /> Refresh
      </button>
    </div>
  </div>

  {#if games.length > 0}
    <GameFilter {games} selected={game} onselect={selectGame} />
  {/if}

  {#if loadError}
    <div class="empty">
      <Icon name="alert" size={32} />
      <p>{loadError}</p>
      <button class="btn" onclick={refresh}><Icon name="retry" size={16} /> Try again</button>
    </div>
  {:else if !loaded}
    <div class="empty"><span class="spinner"></span></div>
  {:else if clips.length === 0}
    <div class="empty">
      <Icon name="film" size={30} />
      <p>{game ? "No clips for this game yet." : "Nobody has shared a clip yet. Upload one from your library!"}</p>
    </div>
  {:else}
    <div class="grid">
      {#each clips as clip (clip.id)}
        <article class="card">
          <button class="thumb" onclick={() => (playing = clip)} aria-label={`Play ${clip.title}`}>
            <img src={clip.thumbnailUrl} alt="" loading="lazy" />
            <span class="play"><Icon name="film" size={22} /></span>
            <span class="dur">{formatDuration(clip.durationMs)}</span>
          </button>
          <div class="body">
            <div class="title" title={clip.title}>{clip.title}</div>
            <div class="meta">
              <span>{clip.owner.username}</span>
              {#if clip.gameName}<span>{clip.gameName}</span>{/if}
              <span>{relativeDate(new Date(clip.createdAt), now)}</span>
            </div>
          </div>
        </article>
      {/each}
    </div>
    {#if cursor}
      <div class="more">
        <button class="btn" onclick={() => load(true)} disabled={loading}>Load older clips</button>
      </div>
    {/if}
  {/if}
</section>

{#if playing}
  <div class="backdrop" onclick={(e) => e.target === e.currentTarget && (playing = null)} role="presentation">
    <div class="player" role="dialog" aria-modal="true" aria-label={playing.title}>
      <div class="player-head">
        <div>
          <div class="title">{playing.title}</div>
          <div class="meta">
            <span>{playing.owner.username}</span>
            {#if playing.gameName}<span>{playing.gameName}</span>{/if}
            <span>{relativeDate(new Date(playing.createdAt), now)}</span>
          </div>
        </div>
        <div class="tools">
          <button class="btn btn-ghost" onclick={() => playing && copyLink(playing)}>
            <Icon name="link" size={16} /> Copy link
          </button>
          <button class="btn btn-ghost" onclick={() => playing && openOnWeb(playing)}>Open on web</button>
          <button class="btn btn-ghost btn-icon" aria-label="Close" onclick={() => (playing = null)}>
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
  .feed {
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
    padding: 1.5rem 1.75rem 2.5rem;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .tools {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  h1 {
    font-size: 1.5rem;
    letter-spacing: -0.02em;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 1rem;
  }

  .card {
    overflow: hidden;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    transition: border-color 0.12s, transform 0.12s;
  }

  .card:hover {
    border-color: var(--accent);
    transform: translateY(-2px);
  }

  .thumb {
    position: relative;
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    padding: 0;
    border: 0;
    background: #000;
    cursor: pointer;
  }

  .thumb img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .play {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: #fff;
    background: rgba(0, 0, 0, 0.35);
    opacity: 0;
    transition: opacity 0.12s;
  }

  .thumb:hover .play,
  .thumb:focus-visible .play {
    opacity: 1;
  }

  .dur {
    position: absolute;
    right: 6px;
    bottom: 6px;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.75);
    color: #fff;
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
  }

  .body {
    padding: 0.6rem 0.75rem 0.75rem;
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
    gap: 0.25rem 0.75rem;
    margin-top: 0.15rem;
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  .more {
    display: flex;
    justify-content: center;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    padding: 5rem 1rem;
    color: var(--text-muted);
    text-align: center;
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 2rem;
    background: rgba(0, 0, 0, 0.75);
  }

  .player {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    width: min(1100px, 100%);
    padding: 1rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }

  .player-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    min-width: 0;
  }

  .player-head > div:first-child {
    min-width: 0;
  }

  video {
    width: 100%;
    max-height: calc(100vh - 12rem);
    background: #000;
    border-radius: var(--radius-sm);
  }
</style>
