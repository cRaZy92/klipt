<script lang="ts">
  import type { GameCount } from "../lib/api";
  import { hueOf } from "../lib/format";

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
      style:--hue={hueOf(game.name)}
      onclick={() => onselect(game.name)}
    >
      <span class="game-dot"></span>
      <span class="name">{game.name}</span>
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
    gap: 0.5rem;
    height: 2.1rem;
    padding: 0 0.6rem 0 0.85rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-muted);
    font-size: 0.88rem;
    font-weight: 600;
    cursor: pointer;
    transition:
      background 0.15s,
      color 0.15s,
      border-color 0.15s,
      box-shadow 0.15s;
  }

  .chip:first-child {
    padding-right: 0.85rem;
  }

  .chip:hover {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .chip.active {
    border-color: hsl(var(--hue, 255) 80% 66% / 0.7);
    background: hsl(var(--hue, 255) 80% 60% / 0.14);
    box-shadow: 0 0 18px -6px hsl(var(--hue, 255) 80% 60% / 0.7);
    color: var(--text);
  }

  .name {
    max-width: 14rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    min-width: 1.4rem;
    padding: 0.05rem 0.4rem;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.07);
    color: var(--text-muted);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  .chip.active .count {
    background: hsl(var(--hue, 255) 80% 60% / 0.3);
    color: var(--text);
  }
</style>
