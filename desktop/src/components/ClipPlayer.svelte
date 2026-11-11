<script lang="ts">
  import { api, errorMessage, type FeedClip } from "../lib/api";
  import { navKey } from "../lib/clipNav";
  import { hueOf, relativeDate, viewsLabel } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  // Plays a clip from the server (the Feed, or your uploads in My clips) over the page. With
  // `onnavigate`, ← / → and the header arrows move to `prev` / `next`, and when the clip ends the
  // next one is offered (never played on its own).
  let {
    clip,
    now,
    onclose,
    prev = null,
    next = null,
    onnavigate,
    own = false,
  }: {
    clip: FeedClip;
    now: number;
    onclose: () => void;
    prev?: FeedClip | null;
    next?: FeedClip | null;
    onnavigate?: (clip: FeedClip) => void;
    /** Your own upload: show which friends watched it. */
    own?: boolean;
  } = $props();

  // The first play of each clip counts as a view (the server ignores your own clips).
  let counted: string | null = null;
  function countView() {
    if (counted === clip.id) return;
    counted = clip.id;
    api.recordView(clip.id).catch(() => {});
  }

  let viewers = $state<string[] | null>(null);
  $effect(() => {
    const id = clip.id;
    viewers = null;
    if (!own) return;
    api
      .clipViewers(id)
      .then((list) => {
        if (clip.id === id) viewers = list.map((v) => v.username);
      })
      .catch(() => {});
  });

  let video = $state<HTMLVideoElement>();
  let ended = $state(false);

  // A new clip starts fresh, without the previous one's "Up next".
  $effect(() => {
    void clip.id;
    ended = false;
  });

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
    api.openWebsite(clip.shareUrl).catch((e) => toast(errorMessage(e), "error"));
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") return onclose();
    const dir = onnavigate ? navKey(e) : null;
    const target = dir === "next" ? next : dir === "prev" ? prev : null;
    if (!target) return;
    // The video would seek on the same key.
    e.preventDefault();
    go(target);
  }
</script>

<svelte:window {onkeydown} />

<div class="backdrop" onclick={(e) => e.target === e.currentTarget && onclose()} role="presentation">
  <div class="player" role="dialog" aria-modal="true" aria-label={clip.title}>
    <div class="player-head">
      <Avatar name={clip.owner.username} size={38} />
      <div class="text">
        <div class="title">{clip.title}</div>
        <div class="meta">
          <span class="who">{clip.owner.username}</span>
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
          <div class="seen-by">With {clip.tagged.join(", ")}</div>
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
        <button class="btn btn-sm" onclick={copyLink}><Icon name="link" size={15} /> Copy link</button>
        <button class="btn btn-sm btn-ghost" onclick={openOnWeb}><Icon name="external" size={15} /> Open on web</button>
        <button class="btn btn-sm btn-ghost btn-icon" aria-label="Close" onclick={onclose}><Icon name="close" /></button>
      </div>
    </div>
    <!-- svelte-ignore a11y_media_has_caption -->
    <div class="stage">
      <video
        bind:this={video}
        src={clip.videoUrl}
        poster={clip.thumbnailUrl}
        controls
        autoplay
        onended={() => (ended = true)}
        onplay={() => {
          ended = false;
          countView();
        }}
      ></video>
      {#if ended && next}
        <div class="up-next" role="group" aria-label="Up next">
          <span class="up-next-label">Up next</span>
          <button class="next-card" onclick={() => go(next)}>
            <img src={next.thumbnailUrl} alt="" />
            <span class="next-text">
              <span class="next-title">{next.title}</span>
              <span class="next-meta">{next.owner.username}{next.gameName ? ` · ${next.gameName}` : ""}</span>
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
</style>
