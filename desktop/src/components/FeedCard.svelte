<script lang="ts">
  import { nameOf, type FeedClip } from "../lib/api";
  import { formatDuration, hueOf, relativeDate, viewsLabel } from "../lib/format";
  import { openProfile, userHover } from "../lib/profiles.svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import ProcessingPill from "./ProcessingPill.svelte";

  // A server clip on the Feed or a profile; `clip` null draws its loading placeholder. The owner's
  // picture and name open their profile and show their hover card, unless `ownerLink` is false
  // (on that person's own profile).
  let {
    clip,
    now = Date.now(),
    index = 0,
    isNew = false,
    ownerLink = true,
    onplay,
    onfavorite,
  }: {
    clip: FeedClip | null;
    now?: number;
    /** Its place in the group, for the staggered rise. */
    index?: number;
    /** Shows the "New" dot. */
    isNew?: boolean;
    ownerLink?: boolean;
    onplay?: () => void;
    onfavorite?: () => void;
  } = $props();
</script>

{#if !clip}
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
{:else}
  {@const owner = nameOf(clip.owner)}
  <article class="card rise" style:--i={index}>
    <button class="thumb" onclick={onplay} aria-label={`Play ${clip.title}`}>
      <img src={clip.thumbnailUrl} alt="" loading="lazy" decoding="async" />
      <span class="play"><Icon name={clip.kind === "screenshot" ? "image" : "play"} size={22} /></span>
      {#if clip.gameName}
        <span class="game-tag" style:--hue={hueOf(clip.gameName)}>
          <span class="game-dot"></span>{clip.gameName}
        </span>
      {/if}
      <ProcessingPill state={clip.processing} />
      <span class="dur">{clip.kind === "screenshot" ? "Screenshot" : formatDuration(clip.durationMs)}</span>
    </button>
    {#if clip.favorite !== undefined && onfavorite}
      <button
        class="star"
        class:on={clip.favorite}
        onclick={onfavorite}
        aria-pressed={clip.favorite}
        aria-label={clip.favorite ? "Remove from favorites" : "Add to favorites"}
        title={clip.favorite ? "In your favorites" : "Add to favorites"}
      >
        <Icon name={clip.favorite ? "starFilled" : "star"} size={16} />
      </button>
    {/if}
    <div class="body">
      {#if ownerLink}
        <button class="avatar-link" tabindex="-1" aria-hidden="true" use:userHover={clip.owner.username} onclick={() => openProfile(clip.owner.username)}>
          <Avatar name={owner} url={clip.owner.avatarUrl} frame={clip.owner.frame} size={32} />
        </button>
      {:else}
        <Avatar name={owner} url={clip.owner.avatarUrl} frame={clip.owner.frame} size={32} />
      {/if}
      <div class="text">
        <div class="title" title={clip.title}>
          {#if isNew}
            <span class="new-dot" role="img" aria-label="New" title="New since you last opened the Feed"></span>
          {/if}
          {clip.title}
        </div>
        <div class="meta">
          {#if ownerLink}
            <button class="who who-link" use:userHover={clip.owner.username} onclick={() => openProfile(clip.owner.username)}>{owner}</button>
          {:else}
            <span class="who">{owner}</span>
          {/if}
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
{/if}

<style>
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
    background: rgba(12, 10, 20, 0.82);
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

  .card {
    position: relative;
    isolation: isolate;
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
    content-visibility: auto;
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
    background: color-mix(in srgb, var(--accent) 90%, transparent);
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
    background: rgba(12, 10, 20, 0.82);
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

  .new-dot {
    display: inline-block;
    width: 0.5rem;
    height: 0.5rem;
    margin-right: 0.3rem;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
    vertical-align: 0.12em;
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

  .avatar-link {
    display: grid;
    flex: none;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    cursor: pointer;
  }

  .who-link {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .who-link:hover,
  .who-link:focus-visible {
    text-decoration: underline;
    text-decoration-color: var(--accent);
    text-underline-offset: 3px;
  }
</style>
