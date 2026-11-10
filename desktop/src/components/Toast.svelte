<script lang="ts">
  import { dismissToast, toasts } from "../lib/toasts.svelte";
  import Icon from "./Icon.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === "error" ? "alert" : "status"}>
      <span class="dot"><Icon name={t.kind === "error" ? "alert" : "check"} size={16} /></span>
      <span class="text">{t.message}</span>
      <button class="close" aria-label="Dismiss" onclick={() => dismissToast(t.id)}>
        <Icon name="close" size={14} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 1.25rem;
    bottom: 1.25rem;
    z-index: 100;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.5rem;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    max-width: 26rem;
    padding: 0.7rem 0.8rem 0.7rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
    box-shadow: var(--shadow);
    pointer-events: auto;
    animation: pop 0.18s ease-out;
  }

  .dot {
    display: grid;
    place-items: center;
    flex: none;
    width: 1.6rem;
    height: 1.6rem;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .success .dot {
    background: var(--success-soft);
    color: var(--success);
  }

  .error .dot {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .text {
    flex: 1;
    font-size: 0.92rem;
  }

  .close {
    display: grid;
    place-items: center;
    padding: 0.25rem;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--text-faint);
    cursor: pointer;
  }

  .close:hover {
    color: var(--text);
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
</style>
