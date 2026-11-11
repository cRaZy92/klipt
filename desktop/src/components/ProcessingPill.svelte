<script lang="ts">
  import Icon from "./Icon.svelte";

  // A small pill on a server clip's thumbnail while the server compresses it to save space. The
  // clip plays either way; this just explains why its size may still change.
  let { state }: { state: "queued" | "running" | null | undefined } = $props();
</script>

{#if state === "queued" || state === "running"}
  <span
    class="proc"
    class:running={state === "running"}
    title={state === "running"
      ? "Compressing on the server to save space. It already plays."
      : "Waiting to be compressed on the server. It already plays."}
  >
    <Icon name={state === "running" ? "spinner" : "clock"} size={12} />
    {state === "running" ? "Processing" : "Queued"}
  </span>
{/if}

<style>
  .proc {
    position: absolute;
    bottom: 0.5rem;
    left: 0.55rem;
    z-index: 2;
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.2rem 0.5rem 0.2rem 0.4rem;
    border: 1px solid rgba(124, 92, 255, 0.35);
    border-radius: 999px;
    background: rgba(20, 14, 40, 0.8);
    backdrop-filter: blur(8px);
    color: var(--accent-text, #c9bcff);
    font-size: 0.72rem;
    font-weight: 700;
    pointer-events: auto;
  }

  .running :global(svg) {
    animation: proc-spin 1s linear infinite;
  }

  @keyframes proc-spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .running :global(svg) {
      animation: none;
    }
  }
</style>
