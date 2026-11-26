<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, errorMessage, nameOf, type FeedClip } from "../lib/api";
  import { navKey } from "../lib/clipNav";
  import { hueOf, relativeDate, viewsLabel } from "../lib/format";
  import { loadPlayerPrefs, playerKey } from "../lib/player";
  import { toast } from "../lib/toasts.svelte";
  import AlbumDialog from "./AlbumDialog.svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import PlayerBar from "./PlayerBar.svelte";

  // Plays a clip from the server (the Feed, or your uploads in My clips) over the page, with
  // Klipt's own controls. With `onnavigate`, ← / → and the header arrows move to `prev` / `next`,
  // and when the clip ends the next one is offered (never played on its own).
  let {
    clip,
    now,
    onclose,
    prev = null,
    next = null,
    onnavigate,
    own = false,
    favorite = false,
    onfavorite,
  }: {
    clip: FeedClip;
    now: number;
    onclose: () => void;
    prev?: FeedClip | null;
    next?: FeedClip | null;
    onnavigate?: (clip: FeedClip) => void;
    /** Your own upload: show which friends watched it. */
    own?: boolean;
    /** Whether the clip is starred (as its card shows it). */
    favorite?: boolean;
    /** Stars or unstars it; without it there's no star. */
    onfavorite?: () => void;
  } = $props();

  // Keyed on these, not on `clip`: a new clip object for the same clip (starred, say) mustn't
  // reload the video.
  const clipId = $derived(clip.id);
  const videoUrl = $derived(clip.videoUrl);
  const isShot = $derived(clip.kind === "screenshot");

  // The first play of each clip counts as a view (the server ignores your own clips).
  let counted: string | null = null;
  function countView() {
    if (counted === clip.id) return;
    counted = clip.id;
    api.recordView(clip.id).catch(() => {});
  }

  let viewers = $state<string[] | null>(null);
  $effect(() => {
    const id = clipId;
    viewers = null;
    if (!own) return;
    api
      .clipViewers(id)
      .then((list) => {
        if (clip.id === id) viewers = list.map((v) => nameOf(v));
      })
      .catch(() => {});
  });

  let video = $state<HTMLVideoElement>();
  let ended = $state(false);
  let paused = $state(true);

  // A new clip starts fresh, without the previous one's "Up next".
  $effect(() => {
    void clipId;
    ended = false;
    wake();
  });

  // The video comes through the app's read-ahead cache (WebKit reads server videos in small
  // requests, each a round trip, so it starts slowly from the server directly); the server's own
  // URL if the cache can't be used.
  let src = $state<string | null>(null);
  let cached = false;
  $effect(() => {
    const direct = videoUrl;
    src = null;
    cached = false;
    if (isShot) return;
    let live = true;
    api.playerVideoUrl(direct).then(
      (url) => {
        if (!live) return;
        src = url;
        cached = true;
      },
      () => {
        if (live) src = direct;
      },
    );
    return () => {
      live = false;
    };
  });

  function onvideoerror() {
    if (!cached) return;
    cached = false;
    src = clip.videoUrl;
  }

  // Started from here: WebKit's `autoplay` doesn't always start it.
  function onloadedmetadata() {
    if (!video) return;
    const prefs = loadPlayerPrefs();
    video.volume = prefs.volume;
    video.muted = prefs.muted;
    video.play().catch(() => {}); // refused or cut short by the next clip: the bar shows Play
  }

  function toggle() {
    if (!video) return;
    if (video.paused) video.play().catch(() => {});
    else video.pause();
  }

  // The bar hides 2 s after the mouse last moved, while playing.
  let idle = $state(false);
  let held = $state(false);
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  function wake() {
    idle = false;
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => (idle = true), 2000);
  }
  const barVisible = $derived(!idle || paused || held);
  $effect(() => () => clearTimeout(idleTimer));

  // Fullscreen: the Klipt window fills the screen and the player fills the window.
  let fullscreen = $state(false);
  /** The window was fullscreen already: leaving the player's fullscreen keeps it that way. */
  let windowWasFullscreen = false;
  async function setFullscreen(on: boolean) {
    if (on === fullscreen) return;
    fullscreen = on;
    try {
      const win = getCurrentWindow();
      if (on) windowWasFullscreen = await win.isFullscreen();
      if (!windowWasFullscreen) await win.setFullscreen(on);
    } catch {
      // The player still fills the window.
    }
  }
  // Closed some other way (the page changed): don't leave the window fullscreen.
  $effect(() => () => {
    if (fullscreen && !windowWasFullscreen) getCurrentWindow().setFullscreen(false).catch(() => {});
  });

  async function close() {
    await setFullscreen(false);
    onclose();
  }

  function go(target: FeedClip | null) {
    if (target && onnavigate) onnavigate(target);
  }

  function replay() {
    ended = false;
    if (!video) return;
    video.currentTime = 0;
    void video.play();
  }

  async function copyLink() {
    try {
      await api.copyText(clip.shareUrl);
      toast("Link copied!", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  function openOnWeb() {
    menuOpen = false;
    api.openWebsite(clip.shareUrl).catch((e) => toast(errorMessage(e), "error"));
  }

  let menuOpen = $state(false);
  let menuWrap = $state<HTMLDivElement>();
  function closeMenuOutside(e: MouseEvent) {
    if (menuOpen && !menuWrap?.contains(e.target as Node)) menuOpen = false;
  }

  /** "Add to album" is open over the player: the player's keys wait. */
  let albumOpen = $state(false);

  function onkeydown(e: KeyboardEvent) {
    if (albumOpen) return;
    const action = playerKey(e);
    if (action === "escape") {
      e.preventDefault();
      if (menuOpen) menuOpen = false;
      else if (fullscreen) void setFullscreen(false);
      else void close();
      return;
    }
    // Typing in a field (or moving the volume slider) keeps its own keys.
    const target = e.target as HTMLElement | null;
    if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
    if (action && video && clip.kind !== "screenshot") {
      e.preventDefault();
      wake();
      if (action === "toggle") toggle();
      else if (action === "mute") video.muted = !video.muted;
      else if (action === "fullscreen") void setFullscreen(!fullscreen);
      else {
        const end = Number.isFinite(video.duration) ? video.duration : video.currentTime + 5;
        video.currentTime = Math.min(end, Math.max(0, video.currentTime + (action === "back" ? -5 : 5)));
      }
      return;
    }
    const dir = onnavigate ? navKey(e) : null;
    const goTo = dir === "next" ? next : dir === "prev" ? prev : null;
    if (!goTo) return;
    // The video would seek on the same key.
    e.preventDefault();
    go(goTo);
  }
</script>

<svelte:window {onkeydown} onclick={closeMenuOutside} />

<div
  class="backdrop"
  class:fullscreen
  onclick={(e) => e.target === e.currentTarget && !fullscreen && close()}
  role="presentation"
>
  <div class="player" class:fullscreen role="dialog" aria-modal="true" aria-label={clip.title}>
    {#if !fullscreen}
      <div class="player-head">
        <Avatar name={nameOf(clip.owner)} url={clip.owner.avatarUrl} frame={clip.owner.frame} size={38} />
        <div class="text">
          <div class="title">{clip.title}</div>
          <div class="meta">
            <span class="who">{nameOf(clip.owner)}</span>
            {#if clip.gameName}
              <span aria-hidden="true">·</span>
              <span class="game"><span class="game-dot" style:--hue={hueOf(clip.gameName)}></span>{clip.gameName}</span>
            {/if}
            <span aria-hidden="true">·</span>
            <span>{relativeDate(new Date(clip.createdAt), now)}</span>
            {#if clip.views > 0}
              <span aria-hidden="true">·</span>
              <span>{viewsLabel(clip.views)}</span>
            {/if}
          </div>
          {#if clip.tagged?.length}
            <div class="seen-by">With {clip.tagged.map((u, i) => clip.taggedNames?.[i] ?? u).join(", ")}</div>
          {/if}
          {#if viewers}
            <div class="seen-by">{viewers.length > 0 ? `Seen by ${viewers.join(", ")}` : "No friends have watched it yet."}</div>
          {/if}
        </div>
        <div class="tools">
          {#if onnavigate}
            <button
              class="btn btn-sm btn-ghost btn-icon"
              aria-label="Previous clip"
              title="Previous clip (←)"
              disabled={!prev}
              onclick={() => go(prev)}><Icon name="back" size={16} /></button>
            <button
              class="btn btn-sm btn-ghost btn-icon"
              aria-label="Next clip"
              title="Next clip (→)"
              disabled={!next}
              onclick={() => go(next)}><Icon name="forward" size={16} /></button>
          {/if}
          {#if onfavorite}
            <button
              class="btn btn-sm btn-ghost btn-icon star"
              class:on={favorite}
              onclick={onfavorite}
              aria-pressed={favorite}
              aria-label={favorite ? "Remove from favorites" : "Add to favorites"}
              title={favorite ? "In your favorites" : "Add to favorites"}
            >
              <Icon name={favorite ? "starFilled" : "star"} size={16} />
            </button>
          {/if}
          <button class="btn btn-sm" onclick={copyLink}><Icon name="link" size={15} /> Copy link</button>
          <button class="btn btn-sm btn-ghost" onclick={() => (albumOpen = true)} title="Put it in one of your albums">
            <Icon name="album" size={15} /> Album
          </button>
          <div class="menu-wrap" bind:this={menuWrap}>
            <button
              class="btn btn-sm btn-ghost btn-icon"
              aria-label="More"
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              onclick={() => (menuOpen = !menuOpen)}><Icon name="more" size={16} /></button>
            {#if menuOpen}
              <div class="menu" role="menu">
                <button role="menuitem" onclick={openOnWeb}><Icon name="external" size={15} /> Open on web</button>
              </div>
            {/if}
          </div>
          <button class="btn btn-sm btn-ghost btn-icon" aria-label="Close" onclick={close}><Icon name="close" /></button>
        </div>
      </div>
    {/if}
    <!-- svelte-ignore a11y_media_has_caption -->
    <div class="stage" class:idle={!barVisible} onpointermove={wake} role="presentation">
      {#if clip.kind === "screenshot"}
        <img class="shot" src={clip.videoUrl} alt={clip.title} onload={countView} />
      {:else}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
        <video
          bind:this={video}
          src={src ?? undefined}
          poster={clip.thumbnailUrl}
          onclick={toggle}
          ondblclick={() => setFullscreen(!fullscreen)}
          {onloadedmetadata}
          onerror={onvideoerror}
          onended={() => (ended = true)}
          onplay={() => {
            ended = false;
            paused = false;
            countView();
            wake();
          }}
          onpause={() => (paused = true)}
        ></video>
        {#if video}
          <PlayerBar
            {video}
            {fullscreen}
            visible={barVisible && !(ended && next)}
            ontogglefullscreen={() => setFullscreen(!fullscreen)}
            onfocuschange={(f) => (held = f)}
          />
        {/if}
      {/if}
      {#if ended && next}
        <div class="up-next" role="group" aria-label="Up next">
          <span class="up-next-label">Up next</span>
          <button class="next-card" onclick={() => go(next)}>
            <img src={next.thumbnailUrl} alt="" />
            <span class="next-text">
              <span class="next-title">{next.title}</span>
              <span class="next-meta">{nameOf(next.owner)}{next.gameName ? ` · ${next.gameName}` : ""}</span>
            </span>
          </button>
          <div class="up-next-actions">
            <button class="btn btn-primary" onclick={() => go(next)}><Icon name="play" size={15} /> Play next</button>
            <button class="btn btn-ghost" onclick={replay}><Icon name="retry" size={15} /> Replay</button>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

{#if albumOpen}
  <AlbumDialog clipIds={[clip.id]} onclose={() => (albumOpen = false)} />
{/if}

<style>
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

  .text {
    flex: 1;
    min-width: 0;
  }

  .title {
    overflow: hidden;
    font-family: var(--font-display);
    font-size: 1.1rem;
    font-weight: 700;
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

  .seen-by {
    margin-top: 0.15rem;
    overflow: hidden;
    color: var(--text-muted);
    font-size: 0.8rem;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  .tools {
    display: flex;
    flex: none;
    align-items: center;
    gap: 0.35rem;
  }

  .stage {
    position: relative;
    overflow: hidden;
    border-radius: var(--radius);
  }

  .shot {
    display: block;
    width: 100%;
    max-height: calc(100vh - 12rem);
    object-fit: contain;
    background: #000;
    border-radius: var(--radius);
  }

  video {
    display: block;
    width: 100%;
    max-height: calc(100vh - 12rem);
    background: #000;
    border-radius: var(--radius);
  }

  .up-next {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.9rem;
    padding: 1rem;
    border-radius: var(--radius);
    background: rgba(5, 4, 10, 0.78);
    backdrop-filter: blur(4px);
    animation: fade 0.2s ease-out;
  }

  .up-next-label {
    color: var(--text-muted);
    font-size: 0.8rem;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .next-card {
    display: flex;
    flex-direction: column;
    width: min(22rem, 100%);
    overflow: hidden;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: border-color 0.2s;
  }

  .next-card:hover {
    border-color: var(--accent);
  }

  .next-card img {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    background: #000;
  }

  .next-text {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
    padding: 0.6rem 0.8rem 0.7rem;
  }

  .next-title {
    overflow: hidden;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .next-meta {
    overflow: hidden;
    color: var(--text-muted);
    font-size: 0.84rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .up-next-actions {
    display: flex;
    gap: 0.5rem;
  }

  /* Fullscreen: only the video and its bar, edge to edge. */
  .backdrop.fullscreen {
    padding: 0;
    background: #000;
    backdrop-filter: none;
  }

  .player.fullscreen {
    width: 100%;
    height: 100%;
    gap: 0;
    padding: 0;
    border: 0;
    border-radius: 0;
    box-shadow: none;
    background: #000;
    animation: none;
  }

  .player.fullscreen .stage {
    flex: 1;
    display: grid;
    place-items: center;
    border-radius: 0;
  }

  .player.fullscreen video {
    width: 100%;
    height: 100%;
    max-height: 100vh;
    border-radius: 0;
  }

  .stage.idle,
  .stage.idle video {
    cursor: none;
  }

  video {
    cursor: pointer;
  }

  .star.on {
    color: #ffc53d;
  }

  .menu-wrap {
    position: relative;
  }

  .menu {
    position: absolute;
    top: calc(100% + 0.4rem);
    right: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 11rem;
    padding: 0.35rem;
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    background: var(--surface-2);
    box-shadow: var(--shadow);
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.5rem 0.6rem;
    border: none;
    border-radius: 8px;
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-size: 0.9rem;
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover {
    background: var(--surface-3);
    color: var(--text);
  }
</style>
