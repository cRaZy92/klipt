<script lang="ts">
  import { hotkeyKeys } from "../lib/format";

  // A shortcut drawn as keycaps: [Alt] + [`].
  let { shortcut, size = "md" }: { shortcut: string; size?: "md" | "lg" } = $props();

  const keys = $derived(hotkeyKeys(shortcut));
</script>

<span class="keys {size}">
  {#each keys as key, i (i)}
    {#if i > 0}<span class="plus">+</span>{/if}
    <!-- Punctuation (` ; ') is drawn bigger: at text size it's a speck on the keycap. -->
    <kbd><span class:glyph={/^[^\p{L}\p{N}]$/u.test(key)}>{key}</span></kbd>
  {/each}
</span>

<style>
  .keys {
    display: inline-flex;
    align-items: center;
    gap: 0.3em;
    vertical-align: middle;
  }

  kbd {
    display: inline-grid;
    place-items: center;
    min-width: 1.9em;
    height: 1.75em;
    padding: 0 0.5em;
    border: 1px solid var(--border-strong);
    border-bottom-width: 3px;
    border-radius: 7px;
    background: linear-gradient(180deg, var(--surface-3), var(--surface-2));
    color: var(--text);
    font-family: inherit;
    font-size: 0.86em;
    font-weight: 700;
    line-height: 1;
  }

  .glyph {
    font-size: 1.35em;
    font-weight: 800;
    line-height: 0.8;
  }

  .plus {
    color: var(--text-faint);
    font-size: 0.85em;
    font-weight: 600;
  }

  .lg {
    gap: 0.5rem;
    font-size: 1.5rem;
  }

  .lg kbd {
    min-width: 2.4em;
    height: 2.2em;
    padding: 0 0.65em;
    border-color: rgba(124, 92, 255, 0.55);
    border-bottom-width: 5px;
    border-radius: 12px;
    background: linear-gradient(180deg, #2c2742, #1d1a2b);
    box-shadow:
      0 10px 30px -8px var(--accent-glow),
      inset 0 1px 0 rgba(255, 255, 255, 0.08);
    font-size: 0.8em;
  }
</style>
