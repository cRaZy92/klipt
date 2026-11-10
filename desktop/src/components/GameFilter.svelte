<script lang="ts">
  import type { GameCount } from "../lib/api";

  let {
    games,
    selected,
    onselect,
  }: { games: GameCount[]; selected: string | null; onselect: (game: string | null) => void } = $props();
</script>

<div class="filter" role="group" aria-label="Filter by game">
  <button class="chip" class:active={selected === null} aria-pressed={selected === null} onclick={() => onselect(null)}>
    All games
  </button>
  {#each games as game (game.name)}
    <button
      class="chip"
      class:active={selected === game.name}
      aria-pressed={selected === game.name}
      onclick={() => onselect(game.name)}
    >
      {game.name}
      <span class="count">{game.count}</span>
    </button>
  {/each}
</div>

<style>
  .filter {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    height: 2rem;
    padding: 0 0.85rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-muted);
    font-size: 0.88rem;
    font-weight: 600;
    cursor: pointer;
    transition:
      background 0.12s,
      color 0.12s,
      border-color 0.12s;
  }

  .chip:hover {
    color: var(--text);
  }

  .chip.active {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }

  .count {
    padding: 0 0.4rem;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--text-muted);
    font-size: 0.75rem;
  }
</style>
