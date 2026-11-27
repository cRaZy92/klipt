<script lang="ts">
  import { untrack } from "svelte";
  import { api, errorCode, errorMessage, type FeedClip, type GameCount } from "../lib/api";
  import { neighbors } from "../lib/clipNav";
  import { feedNeedsRefresh, isNewClip, type FeedLoaded } from "../lib/feedRefresh";
  import { groupByDay } from "../lib/format";
  import { stickyTop } from "../lib/stickyTop";
  import { toast } from "../lib/toasts.svelte";
  import ClipPlayer from "../components/ClipPlayer.svelte";
  import FeedCard from "../components/FeedCard.svelte";
  import GameFilter from "../components/GameFilter.svelte";
  import Icon, { type IconName } from "../components/Icon.svelte";
  import SearchInput from "../components/SearchInput.svelte";

  let {
    active = true,
    server = null,
    me = null,
    onunauthorized,
  }: {
    /** This is the screen shown (it's kept alive, hidden, while another one is). */
    active?: boolean;
    /** The server's host name, shown under the heading. */
    server?: string | null;
    /** Your username: your own clips are never "new". */
    me?: string | null;
    onunauthorized: () => void;
  } = $props();

  let clips = $state<FeedClip[]>([]);
  /** What you'd seen of the feed before this visit: clips after it get the "New" dot. */
  let seenBefore = $state<string | null>(null);
  let games = $state<GameCount[]>([]);
  let game = $state<string | null>(null);
  let search = $state("");
  /** Every clip, only ones you're tagged in, or only ones you starred (like My clips' places). */
  let show = $state<"all" | "withMe" | "favorites">("all");
  const withMe = $derived(show === "withMe");
  const favoritesOnly = $derived(show === "favorites");
  /** Only video clips or only screenshots. */
  let kind = $state<"clip" | "screenshot" | null>(null);
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

  // Nearing the end of the list loads the next page by itself. `nearEnd` stays true while the
  // marker is in view, so a short page that leaves it in view loads another. A failed page waits
  // for the button instead of retrying in a loop.
  let nearEnd = $state(false);
  let moreFailed = $state(false);
  $effect(() => {
    if (!active || !nearEnd || !cursor || loading || moreFailed || playing) return;
    untrack(() => void load(true));
  });

  /** `use:watchEnd`: reports whether the list's end is within ~1.5 screens of being scrolled to. */
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
    moreFailed = false;
    try {
      const page = await api.feedClips(game, more ? cursor : null, search.trim() || null, withMe, kind, favoritesOnly);
      if (id !== request) return;
      clips = more ? [...clips, ...page.items] : page.items;
      cursor = page.nextCursor;
      // The unfiltered first page is the whole feed's newest end: you've now seen it.
      if (!more && !game && !search.trim() && !withMe && !kind && !favoritesOnly && page.items[0]) {
        api
          .markFeedSeen(page.items[0].createdAt)
          .then((before) => (seenBefore = before))
          .catch(() => {});
      }
      now = Date.now();
      shown = more ? { ...shown, pages: shown.pages + 1 } : { loadedAt: now, pages: 1 };
      loaded = true;
    } catch (e) {
      if (id !== request) return;
      const message = failed(e);
      if (more) {
        moreFailed = true;
        toast(message, "error");
      }
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

  const filtered = $derived(!!(game || search.trim() || withMe || kind || favoritesOnly));

  /** Puts every filter back to showing the whole feed. */
  function clearFilters() {
    game = null;
    search = "";
    show = "all";
    kind = null;
    void load();
  }

  function selectGame(g: string | null) {
    game = g;
    void load();
  }

  /** The playing clip as the list has it now, so the player's star follows its card. */
  const playingNow = $derived(playing ? (clips.find((c) => c.id === playing!.id) ?? playing) : null);

  /** Stars or unstars a clip: shown at once, put back if the server refuses. */
  async function toggleFavorite(clip: FeedClip) {
    const favorite = !clip.favorite;
    const set = (on: boolean) => (clips = clips.map((c) => (c.id === clip.id ? { ...c, favorite: on } : c)));
    set(favorite);
    // The player keeps showing it even once it leaves the Favorites list.
    if (playing?.id === clip.id) playing = { ...playing, favorite };
    try {
      await api.setServerFavorite(clip.id, favorite);
      // Unstarred while viewing favorites: it no longer belongs here.
      if (favoritesOnly && !favorite) clips = clips.filter((c) => c.id !== clip.id);
    } catch (e) {
      set(!favorite);
      if (playing?.id === clip.id) playing = { ...playing, favorite: !favorite };
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

  /** What's loaded; decides whether showing the Feed again refreshes it (see feedRefresh.ts). */
  let shown: FeedLoaded = { loadedAt: 0, pages: 0 };
  $effect(() => {
    if (active) untrack(() => feedNeedsRefresh(shown, Date.now()) && refresh());
  });

  let searchInput = $state<HTMLInputElement>();

  /** `/` jumps to the search field (not while typing or while a clip plays over the list). */
  function onkeydown(e: KeyboardEvent) {
    if (!active || playing || e.key !== "/" || e.ctrlKey || e.metaKey || e.altKey) return;
    if ((e.target as HTMLElement | null)?.closest("input, textarea, select, [contenteditable]")) return;
    e.preventDefault();
    searchInput?.focus();
    searchInput?.select();
  }

  // Hidden: stop anything playing.
  $effect(() => {
    if (!active) playing = null;
  });

  const SHOWS: { value: typeof show; label: string; title?: string; icon?: IconName }[] = [
    { value: "all", label: "All" },
    { value: "withMe", label: "I'm in it", title: "Clips your friends tagged you in" },
    { value: "favorites", label: "Favorites", title: "Clips you starred", icon: "star" },
  ];

  const MEDIA: { value: typeof kind; label: string }[] = [
    { value: null, label: "Everything" },
    { value: "clip", label: "Clips" },
    { value: "screenshot", label: "Screenshots" },
  ];
</script>

<section class="page">
  <div class="page-top" use:stickyTop>
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
      <div class="segmented" role="group" aria-label="Show clips">
        {#each SHOWS as sh (sh.value)}
          <button
            class:active={show === sh.value}
            aria-pressed={show === sh.value}
            title={sh.title}
            onclick={() => {
              show = sh.value;
              void load();
            }}
          >
            {#if sh.icon}<Icon name={sh.icon} size={15} />{/if}
            {sh.label}
          </button>
        {/each}
      </div>
      <div class="segmented" role="group" aria-label="Clips or screenshots">
        {#each MEDIA as m (m.label)}
          <button
            class:active={kind === m.value}
            aria-pressed={kind === m.value}
            onclick={() => {
              kind = m.value;
              void load();
            }}
          >
            {m.label}
          </button>
        {/each}
      </div>
      <SearchInput
        bind:value={search}
        bind:input={searchInput}
        shortcut="/"
        placeholder="Search by title, game or friend"
        label="Search the feed"
        delay={300}
        onsearch={() => void load()}
      />
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
        <FeedCard clip={null} />
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
      {:else if kind}
        <p>No {kind === "screenshot" ? "screenshots" : "clips"} shared yet.</p>
      {:else}
        <h2>Nothing shared yet</h2>
        <p>Upload a clip from My clips and it shows up here for everyone.</p>
      {/if}
      {#if filtered}
        <button class="btn" onclick={clearFilters}><Icon name="close" size={16} /> Clear filters</button>
      {/if}
    </div>
  {:else}
    <div class="clip-groups">
      {#each groups as group (group.key)}
        <section aria-label={group.label}>
          <h2 class="group-label">{group.label}</h2>
          <div class="clip-grid">
            {#each group.items as clip, i (clip.id)}
              <FeedCard
                {clip}
                {now}
                index={i}
                isNew={isNewClip(clip, seenBefore, me)}
                onplay={() => (playing = clip)}
                onfavorite={() => toggleFavorite(clip)}
              />
            {/each}
          </div>
        </section>
      {/each}
    </div>
    {#if cursor}
      <div class="more" use:watchEnd>
        {#if moreFailed}
          <button class="btn" onclick={() => load(true)} disabled={loading}>
            <Icon name="retry" size={16} /> Load older clips
          </button>
        {:else}
          <span class="spinner" role="status" aria-label="Loading older clips"></span>
        {/if}
      </div>
    {/if}
  {/if}
</section>

<svelte:window {onkeydown} />

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
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem 1rem;
    margin-bottom: 0.75rem;
  }


  .refresh-icon {
    display: inline-grid;
  }

  .spinning {
    animation: spin 0.9s linear infinite;
  }

  .more {
    display: flex;
    justify-content: center;
    min-height: 2.5rem;
    padding-top: 1.5rem;
    color: var(--text-muted);
  }
</style>
