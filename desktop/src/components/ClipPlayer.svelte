<script lang="ts">
  import { api, errorMessage, type FeedClip } from "../lib/api";
  import { hueOf, relativeDate } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  // Plays a clip from the server (the Feed, or your uploads in My clips) over the page.
  let { clip, now, onclose }: { clip: FeedClip; now: number; onclose: () => void } = $props();

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
    if (e.key === "Escape") onclose();
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
        </div>
      </div>
      <div class="tools">
        <button class="btn btn-sm" onclick={copyLink}><Icon name="link" size={15} /> Copy link</button>
        <button class="btn btn-sm btn-ghost" onclick={openOnWeb}><Icon name="external" size={15} /> Open on web</button>
        <button class="btn btn-sm btn-ghost btn-icon" aria-label="Close" onclick={onclose}><Icon name="close" /></button>
      </div>
    </div>
    <!-- svelte-ignore a11y_media_has_caption -->
    <video src={clip.videoUrl} poster={clip.thumbnailUrl} controls autoplay></video>
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

  video {
    width: 100%;
    max-height: calc(100vh - 12rem);
    background: #000;
    border-radius: var(--radius);
  }
</style>
