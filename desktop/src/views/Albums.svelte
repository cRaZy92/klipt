<script lang="ts">
  import { untrack } from "svelte";
  import { api, errorCode, errorMessage, nameOf, type Album, type AlbumSummary, type FeedClip } from "../lib/api";
  import { neighbors } from "../lib/clipNav";
  import { formatDuration, relativeDate } from "../lib/format";
  import { stickyTop } from "../lib/stickyTop";
  import { toast } from "../lib/toasts.svelte";
  import ClipPlayer from "../components/ClipPlayer.svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";

  // Your albums on the server: a grid of them, and one opened to play, reorder, take clips out,
  // rename, share or delete. Clips go in from My clips (select → Add to album) or a clip's page.

  let {
    active = true,
    onunauthorized,
  }: {
    /** This is the screen shown (it's kept alive, hidden, while another one is). */
    active?: boolean;
    onunauthorized: () => void;
  } = $props();

  let albums = $state<AlbumSummary[]>([]);
  let loaded = $state(false);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let now = $state(Date.now());

  /** The album opened (null: the grid). */
  let open = $state<Album | null>(null);
  let opening = $state<string | null>(null);
  /** Clip ids a change is being sent for. */
  let busy = $state<Set<string>>(new Set());
  let playing = $state<FeedClip | null>(null);
  const around = $derived(playing && open ? neighbors(open.clips, playing.id) : { prev: null, next: null });

  let newTitle = $state("");
  let creating = $state(false);
  let renaming = $state<string | null>(null);
  let deleting = $state(false);

  const clipsLabel = (n: number) => (n === 1 ? "1 clip" : `${n} clips`);
  const totalMs = $derived(open ? open.clips.reduce((sum, c) => sum + (c.kind === "screenshot" ? 0 : c.durationMs), 0) : 0);

  function failed(e: unknown, what: string) {
    if (errorCode(e) === "unauthorized") onunauthorized();
    else toast(`${what}: ${errorMessage(e)}`, "error");
  }

  async function load() {
    loading = true;
    loadError = null;
    try {
      albums = await api.listAlbums();
      now = Date.now();
      loaded = true;
    } catch (e) {
      if (errorCode(e) === "unauthorized") onunauthorized();
      loadError = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  // Loaded when first shown, and again each time the screen comes back (albums change on the website too).
  $effect(() => {
    if (!active) return;
    untrack(() => {
      void load();
      if (open) void reopen(open.id);
    });
  });

  async function reopen(id: string) {
    try {
      open = await api.getAlbum(id);
    } catch (e) {
      if (errorCode(e) === "not_found") {
        open = null;
        return;
      }
      failed(e, "Couldn't load the album");
    }
  }

  async function show(album: AlbumSummary) {
    opening = album.id;
    try {
      open = await api.getAlbum(album.id);
      now = Date.now();
    } catch (e) {
      failed(e, "Couldn't open the album");
    } finally {
      opening = null;
    }
  }

  function close() {
    open = null;
    void load();
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    const title = newTitle.trim();
    if (!title) return;
    creating = true;
    try {
      const made = await api.createAlbum(title, []);
      newTitle = "";
      albums = [made, ...albums];
      await show(made);
    } catch (e) {
      failed(e, "Couldn't make the album");
    } finally {
      creating = false;
    }
  }

  async function copyLink(url: string) {
    try {
      await api.copyText(url);
      toast("Album link copied!", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  function openOnWebsite(url: string) {
    api.openWebsite(url).catch((e) => toast(errorMessage(e), "error"));
  }

  async function withClip(id: string, work: () => Promise<void>) {
    busy = new Set([...busy, id]);
    try {
      await work();
    } finally {
      busy = new Set([...busy].filter((b) => b !== id));
    }
  }

  function move(clip: FeedClip, by: -1 | 1) {
    const album = open;
    if (!album) return;
    const ids = album.clips.map((c) => c.id);
    const from = ids.indexOf(clip.id);
    const to = from + by;
    if (from < 0 || to < 0 || to >= ids.length) return;
    ids.splice(to, 0, ids.splice(from, 1)[0]);
    void withClip(clip.id, async () => {
      try {
        open = await api.updateAlbum(album.id, { clipIds: ids });
      } catch (e) {
        if (errorCode(e) === "album_changed") {
          toast("The album changed meanwhile, so here it is as it is now.", "info");
          await reopen(album.id);
        } else failed(e, "Couldn't move the clip");
      }
    });
  }

  function remove(clip: FeedClip) {
    const album = open;
    if (!album) return;
    void withClip(clip.id, async () => {
      try {
        await api.setAlbumClip(album.id, clip.id, false);
        if (open?.id === album.id) {
          open.clips = open.clips.filter((c) => c.id !== clip.id);
          open.clipCount = open.clips.length;
        }
        if (playing?.id === clip.id) playing = null;
        toast(`Removed “${clip.title}” from the album`);
      } catch (e) {
        failed(e, "Couldn't remove the clip");
      }
    });
  }

  async function rename(e: SubmitEvent) {
    e.preventDefault();
    const album = open;
    const title = renaming?.trim();
    if (!album || !title) return;
    try {
      open = await api.updateAlbum(album.id, { title });
      renaming = null;
    } catch (e) {
      failed(e, "Couldn't rename the album");
    }
  }

  async function confirmDelete() {
    const album = open;
    if (!album) return;
    try {
      await api.deleteAlbum(album.id);
      deleting = false;
      albums = albums.filter((a) => a.id !== album.id);
      open = null;
      toast(`Deleted “${album.title}”. Its clips stay online.`);
    } catch (e) {
      failed(e, "Couldn't delete the album");
    }
  }
</script>

<section class="page">
  {#if !open}
    <div class="page-top" use:stickyTop>
      <header class="page-head">
        <div>
          <h1>Albums</h1>
          <p class="subtitle">Clips, yours or your friends', under one link. Anyone with the link can watch.</p>
        </div>
        <div class="page-tools">
          <form class="new" onsubmit={create}>
            <input class="input" bind:value={newTitle} maxlength="100" placeholder="Name a new album" aria-label="New album name" />
            <button class="btn btn-primary" type="submit" disabled={!newTitle.trim() || creating}>
              <Icon name="album" size={16} /> New album
            </button>
          </form>
          <button class="btn" onclick={load} disabled={loading} aria-label="Refresh" title="Refresh">
            <span class:spinning={loading}><Icon name="retry" size={16} /></span>
          </button>
        </div>
      </header>
    </div>

    {#if loadError && !loaded}
      <div class="empty">
        <div class="empty-icon error"><Icon name="alert" size={28} /></div>
        <p>Couldn't load your albums: {loadError}</p>
        <button class="btn" onclick={load}><Icon name="retry" size={16} /> Try again</button>
      </div>
    {:else if !loaded}
      <p class="muted loading"><span class="spinner" aria-hidden="true"></span> Loading your albums…</p>
    {:else if albums.length === 0}
      <div class="empty">
        <div class="empty-icon"><Icon name="album" size={28} /></div>
        <h2>No albums yet</h2>
        <p>Name one above, or select uploaded clips in My clips and press Add to album.</p>
      </div>
    {:else}
      <div class="album-grid">
        {#each albums as album, i (album.id)}
          <button class="album-card rise" style:--i={i} onclick={() => show(album)} disabled={opening === album.id}>
            <span class="cover">
              {#if album.coverUrl}
                <img src={album.coverUrl} alt="" loading="lazy" decoding="async" />
              {:else}
                <span class="cover-empty"><Icon name="album" size={26} /></span>
              {/if}
              <span class="count">{clipsLabel(album.clipCount)}</span>
              {#if opening === album.id}<span class="cover-busy"><span class="spinner"></span></span>{/if}
            </span>
            <span class="album-title" title={album.title}>{album.title}</span>
            <span class="album-meta">Changed {relativeDate(new Date(album.updatedAt), now)}</span>
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    {@const album = open}
    <div class="detail-top">
      <button class="btn btn-ghost back" onclick={close}><Icon name="back" size={16} /> Albums</button>
      <header class="detail-head">
        <div class="detail-text">
          <h1 title={album.title}>{album.title}</h1>
          <p class="detail-meta">
            <span>{clipsLabel(album.clips.length)}</span>
            {#if totalMs > 0}<span>{formatDuration(totalMs)} in total</span>{/if}
            <span>Changed {relativeDate(new Date(album.updatedAt), now)}</span>
          </p>
        </div>
        <div class="detail-tools">
          <button class="btn btn-primary" onclick={() => copyLink(album.shareUrl)}><Icon name="link" size={16} /> Copy link</button>
          <button class="btn" onclick={() => openOnWebsite(album.shareUrl)}><Icon name="external" size={16} /> Open on website</button>
          <button class="btn btn-ghost" onclick={() => (renaming = album.title)}><Icon name="rename" size={16} /> Rename</button>
          <button class="btn btn-ghost danger" onclick={() => (deleting = true)}><Icon name="trash" size={16} /> Delete</button>
        </div>
      </header>
    </div>

    {#if album.clips.length === 0}
      <div class="empty">
        <div class="empty-icon"><Icon name="album" size={28} /></div>
        <h2>This album is empty</h2>
        <p>In My clips, select uploaded clips and press Add to album. Friends' clips can be added from their page on the website.</p>
      </div>
    {:else}
      <ol class="clip-list">
        {#each album.clips as clip, i (clip.id)}
          <li class:busy={busy.has(clip.id)}>
            <span class="num">{i + 1}</span>
            <button class="thumb" onclick={() => (playing = clip)} aria-label="Play {clip.title}">
              <img src={clip.thumbnailUrl} alt="" loading="lazy" decoding="async" />
              <span class="play"><Icon name={clip.kind === "screenshot" ? "image" : "play"} size={18} /></span>
              <span class="dur">{clip.kind === "screenshot" ? "Shot" : formatDuration(clip.durationMs)}</span>
            </button>
            <button class="text" onclick={() => (playing = clip)}>
              <span class="title" title={clip.title}>{clip.title}</span>
              <span class="meta">
                <span>{nameOf(clip.owner)}</span>
                {#if clip.gameName}<span>{clip.gameName}</span>{/if}
              </span>
            </button>
            <span class="tools">
              <button class="btn btn-sm btn-ghost btn-icon" onclick={() => move(clip, -1)} disabled={i === 0 || busy.size > 0} aria-label="Move up" title="Move up">
                <span class="arrow up"><Icon name="back" size={15} /></span>
              </button>
              <button class="btn btn-sm btn-ghost btn-icon" onclick={() => move(clip, 1)} disabled={i === album.clips.length - 1 || busy.size > 0} aria-label="Move down" title="Move down">
                <span class="arrow down"><Icon name="back" size={15} /></span>
              </button>
              <button class="btn btn-sm btn-ghost btn-icon danger" onclick={() => remove(clip)} disabled={busy.has(clip.id)} aria-label="Remove from the album" title="Remove from the album">
                <Icon name="close" size={15} />
              </button>
            </span>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
</section>

{#if playing}
  <ClipPlayer clip={playing} {now} prev={around.prev} next={around.next} onnavigate={(c) => (playing = c)} onclose={() => (playing = null)} />
{/if}

{#if renaming !== null && open}
  <Modal title="Rename album" onclose={() => (renaming = null)}>
    <form id="rename-album" onsubmit={rename}>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input wide" bind:value={renaming} maxlength="100" aria-label="Album name" autofocus />
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (renaming = null)}>Cancel</button>
      <button class="btn btn-primary" type="submit" form="rename-album" disabled={!renaming?.trim()}>Rename</button>
    {/snippet}
  </Modal>
{/if}

{#if deleting && open}
  <Modal title="Delete “{open.title}”?" onclose={() => (deleting = false)}>
    <p>The album's link stops working. Its clips stay online and in My clips.</p>
    {#snippet footer()}
      <button class="btn" onclick={() => (deleting = false)}>Cancel</button>
      <button class="btn btn-danger" onclick={confirmDelete}>Delete album</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .new {
    display: flex;
    gap: 8px;
  }

  .new .input {
    width: 14rem;
  }

  .spinning {
    display: inline-flex;
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .muted {
    color: var(--text-muted);
  }

  .loading {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 2rem 0;
  }

  .album-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 1.6rem 1.2rem;
    padding-top: 0.6rem;
  }

  .album-card {
    display: grid;
    gap: 0.2rem;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  /* The first clip, with the next ones peeking out behind it like a stack. */
  .cover {
    position: relative;
    display: block;
    aspect-ratio: 16 / 9;
    margin: 10px 10px 0.45rem 0;
    border-radius: var(--radius);
    background: var(--surface-2);
    box-shadow:
      6px -6px 0 -2px var(--surface-3),
      12px -12px 0 -4px var(--surface-2);
    transition: transform 0.18s var(--ease-out), box-shadow 0.18s;
  }

  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    border-radius: var(--radius);
  }

  .album-card:hover .cover,
  .album-card:focus-visible .cover {
    transform: translateY(-2px);
    box-shadow:
      6px -6px 0 -2px var(--surface-3),
      12px -12px 0 -4px var(--surface-2),
      0 0 0 2px var(--accent);
  }

  .cover-empty,
  .cover-busy {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--text-faint);
    border-radius: var(--radius);
  }

  .cover-busy {
    background: rgba(0, 0, 0, 0.45);
  }

  .count {
    position: absolute;
    right: 8px;
    bottom: 8px;
    padding: 2px 8px;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.72);
    color: #fff;
    font-size: 0.78rem;
    font-weight: 650;
  }

  .album-title {
    font-weight: 650;
    font-size: 1rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .album-meta {
    color: var(--text-faint);
    font-size: 0.85rem;
  }

  .detail-top {
    padding: 1.4rem 0 1rem;
  }

  .back {
    margin-left: -0.6rem;
    margin-bottom: 0.6rem;
  }

  .detail-head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: 1rem 1.5rem;
  }

  .detail-text {
    min-width: 0;
    flex: 1 1 18rem;
  }

  .detail-text h1 {
    margin: 0 0 0.35rem;
    overflow-wrap: anywhere;
  }

  .detail-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.2rem 1rem;
    margin: 0;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .detail-tools {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .danger:hover:not(:disabled) {
    color: var(--danger-text);
    background: var(--danger-soft);
  }

  .clip-list {
    list-style: none;
    margin: 0;
    padding: 0.4rem;
    display: grid;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
  }

  .clip-list li {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    padding: 0.45rem 0.6rem 0.45rem 0.3rem;
    border-radius: var(--radius-sm);
    transition: opacity 0.15s;
  }

  .clip-list li:hover {
    background: var(--surface-2);
  }

  .clip-list li.busy {
    opacity: 0.55;
  }

  .num {
    width: 2.4ch;
    flex: none;
    text-align: right;
    color: var(--text-faint);
    font-size: 0.88rem;
    font-variant-numeric: tabular-nums;
  }

  .thumb {
    position: relative;
    flex: none;
    width: 160px;
    aspect-ratio: 16 / 9;
    padding: 0;
    border: 0;
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface-3);
    cursor: pointer;
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .play {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: #fff;
    background: rgba(0, 0, 0, 0.35);
    opacity: 0;
    transition: opacity 0.15s;
  }

  .thumb:hover .play,
  .thumb:focus-visible .play {
    opacity: 1;
  }

  .dur {
    position: absolute;
    right: 5px;
    bottom: 5px;
    padding: 1px 6px;
    border-radius: 5px;
    background: rgba(0, 0, 0, 0.72);
    color: #fff;
    font-size: 0.74rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 0.25rem;
    padding: 0.2rem 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .title {
    font-weight: 600;
    line-height: 1.3;
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow-wrap: anywhere;
  }

  .text:hover .title {
    color: var(--accent-text);
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0 0.9rem;
    color: var(--text-faint);
    font-size: 0.84rem;
  }

  .tools {
    display: flex;
    gap: 2px;
    flex: none;
    opacity: 0.35;
    transition: opacity 0.15s;
  }

  .clip-list li:hover .tools,
  .clip-list li:focus-within .tools {
    opacity: 1;
  }

  .arrow {
    display: inline-flex;
  }

  .arrow.up {
    transform: rotate(90deg);
  }

  .arrow.down {
    transform: rotate(-90deg);
  }

  .wide {
    width: 100%;
  }

  @media (prefers-reduced-motion: reduce) {
    .cover,
    .spinning {
      transition: none;
      animation: none;
    }
  }
</style>
