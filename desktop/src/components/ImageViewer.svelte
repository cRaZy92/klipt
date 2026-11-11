<script lang="ts">
  import Icon from "./Icon.svelte";

  // A screenshot over the page. Escape or a click outside the picture closes it.
  let { src, title, onclose }: { src: string; title: string; onclose: () => void } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window {onkeydown} />

<div class="backdrop" onclick={(e) => e.target === e.currentTarget && onclose()} role="presentation">
  <div class="viewer" role="dialog" aria-modal="true" aria-label={title}>
    <div class="head">
      <div class="title">{title}</div>
      <button class="btn btn-sm btn-ghost btn-icon" aria-label="Close" onclick={onclose}><Icon name="close" /></button>
    </div>
    <img {src} alt={title} />
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
    background: rgba(5, 4, 10, 0.86);
    backdrop-filter: blur(6px);
  }

  .viewer {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    max-width: 100%;
    max-height: 100%;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .title {
    overflow: hidden;
    font-family: var(--font-display);
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  img {
    display: block;
    max-width: 100%;
    max-height: calc(100vh - 7rem);
    border-radius: var(--radius);
    background: #000;
    object-fit: contain;
  }
</style>
