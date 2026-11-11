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
    gap: 0.65rem;
    max-width: 26rem;
    padding: 0.6rem 0.6rem 0.6rem 0.65rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: rgba(30, 28, 42, 0.92);
    backdrop-filter: blur(12px);
    box-shadow: var(--shadow);
    pointer-events: auto;
    animation: slide-in 0.25s var(--ease-out);
  }

  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateX(14px);
    }
  }

  .dot {
    display: grid;
    place-items: center;
    flex: none;
    width: 1.75rem;
    height: 1.75rem;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent-text);
  }

  .success .dot {
    background: var(--success-soft);
    color: var(--success);
  }

  .error {
    border-color: rgba(255, 93, 108, 0.35);
  }

  .error .dot {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .text {
    flex: 1;
    font-size: 0.92rem;
    font-weight: 500;
  }

  .close {
    display: grid;
    place-items: center;
    padding: 0.3rem;
    border: none;
    border-radius: 7px;
    background: none;
    color: var(--text-faint);
    cursor: pointer;
  }

  .close:hover {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text);
  }
</style>
