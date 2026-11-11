<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import { trapFocus } from "../lib/a11y";

  let {
    title,
    onclose,
    children,
    footer,
    width = "28rem",
  }: { title: string; onclose: () => void; children: Snippet; footer?: Snippet; width?: string } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window {onkeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" style:width use:trapFocus>
    <header>
      <h2>{title}</h2>
      <button class="btn btn-ghost btn-icon" aria-label="Close" onclick={onclose}><Icon name="close" /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}
      <footer>{@render footer()}</footer>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 1.5rem;
    background: rgba(5, 4, 10, 0.72);
    backdrop-filter: blur(6px);
    animation: fade 0.15s ease-out;
  }

  .modal {
    max-width: 100%;
    max-height: 100%;
    overflow: auto;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: linear-gradient(180deg, var(--surface-2), var(--surface) 70%);
    box-shadow: var(--shadow);
    animation: pop 0.2s var(--ease-out);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1.1rem 0.8rem 0.25rem 1.4rem;
  }

  h2 {
    font-size: 1.25rem;
    font-weight: 700;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 0.75rem 1.4rem 1.4rem;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    padding: 0 1.4rem 1.4rem;
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
  }
</style>
