<script lang="ts">
  import type { GameCount } from "../lib/api";
  import { menu } from "../lib/a11y";
  import { fitChips } from "../lib/chipFit";
  import { hueOf } from "../lib/format";
  import Icon from "./Icon.svelte";

  let {
    games,
    selected,
    onselect,
  }: { games: GameCount[]; selected: string | null; onselect: (game: string | null) => void } = $props();

  // One line of chips; whatever doesn't fit goes behind "+N more". A hidden selection is pinned
  // next to it so the active filter is always on screen. Widths come from an invisible copy of
  // every chip, re-measured when the row resizes or the games change.

  let row = $state<HTMLElement>();
  let measure = $state<HTMLElement>();
  let fit = $state(Infinity);
  let open = $state(false);
  let query = $state("");
  let moreWrap = $state<HTMLElement>();

  const selectedIndex = $derived(games.findIndex((g) => g.name === selected));
  const shown = $derived(games.slice(0, fit));
  const pinned = $derived(selectedIndex >= fit ? games[selectedIndex] : null);
  const hidden = $derived(games.slice(fit).filter((g) => g !== pinned));
  const matches = $derived(
    query.trim() ? hidden.filter((g) => g.name.toLowerCase().includes(query.trim().toLowerCase())) : hidden,
  );

  function relayout() {
    if (!row || !measure) return;
    const chips = [...measure.children] as HTMLElement[];
    const more = chips.pop()!;
    const all = chips.shift()!;
    fit = fitChips({
      widths: chips.map((c) => c.getBoundingClientRect().width),
      allWidth: all.getBoundingClientRect().width,
      moreWidth: more.getBoundingClientRect().width,
      gap: parseFloat(getComputedStyle(row).columnGap) || 0,
      available: row.clientWidth,
      selected: selectedIndex,
    });
  }

  $effect(() => {
    void games;
    void selectedIndex;
    relayout();
  });

  $effect(() => {
    if (!row || !measure) return;
    const ro = new ResizeObserver(relayout);
    ro.observe(row);
    ro.observe(measure);
    return () => ro.disconnect();
  });

  $effect(() => {
    if (hidden.length === 0) open = false;
  });

  function toggle() {
    open = !open;
    query = "";
  }

  function pick(name: string) {
    open = false;
    onselect(name);
  }

  function closeOutside(e: MouseEvent) {
    if (open && !moreWrap?.contains(e.target as Node)) open = false;
  }

  // After `use:menu` has focused the first item, put the cursor in the search field instead.
  function focusLater(node: HTMLInputElement) {
    queueMicrotask(() => node.focus());
  }
</script>

<svelte:window onclick={closeOutside} />

{#snippet chip(game: GameCount)}
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
{/snippet}

<div class="filter" role="group" aria-label="Filter by game" bind:this={row}>
  <button class="chip all" class:active={selected === null} aria-pressed={selected === null} onclick={() => onselect(null)}>
    All games
  </button>
  {#each shown as game (game.name)}
    {@render chip(game)}
  {/each}
  {#if pinned}
    {@render chip(pinned)}
  {/if}
  {#if hidden.length > 0}
    <div class="more-wrap" bind:this={moreWrap}>
      <button class="chip more" class:open aria-haspopup="menu" aria-expanded={open} onclick={toggle}>
        +{hidden.length} more <Icon name="chevronDown" size={14} />
      </button>
      {#if open}
        <div class="menu" role="menu" aria-label="More games" use:menu={{ close: () => (open = false) }}>
          {#if hidden.length > 6}
            <div class="search">
              <Icon name="search" size={14} />
              <input bind:value={query} placeholder="Find a game" aria-label="Find a game" use:focusLater />
            </div>
          {/if}
          <div class="items">
            {#each matches as game (game.name)}
              <button role="menuitem" style:--hue={hueOf(game.name)} onclick={() => pick(game.name)}>
                <span class="game-dot"></span>
                <span class="name">{game.name}</span>
                <span class="count">{game.count}</span>
              </button>
            {:else}
              <p class="none">No games match “{query.trim()}”.</p>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <div class="measure-box" aria-hidden="true">
    <div class="measure" bind:this={measure}>
      <span class="chip all">All games</span>
      {#each games as game (game.name)}
        <span class="chip">
          <span class="game-dot"></span>
          <span class="name">{game.name}</span>
          <span class="count">{game.count}</span>
        </span>
      {/each}
      <span class="chip more">+{games.length} more <Icon name="chevronDown" size={14} /></span>
    </div>
  </div>
</div>

<style>
  .filter {
    position: relative;
    display: flex;
    flex-wrap: nowrap;
    gap: 0.5rem;
    min-width: 0;
  }

  /* Zero-size and clipped, so the full-length copy can't widen the page. */
  .measure-box {
    position: absolute;
    top: 0;
    left: 0;
    width: 0;
    height: 0;
    overflow: hidden;
  }

  .measure {
    display: flex;
    width: max-content;
    gap: 0.5rem;
    white-space: nowrap;
    visibility: hidden;
    pointer-events: none;
  }

  .chip {
    display: inline-flex;
    flex-shrink: 0;
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
    white-space: nowrap;
    cursor: pointer;
    transition:
      background 0.15s,
      color 0.15s,
      border-color 0.15s,
      box-shadow 0.15s;
  }

  .chip.all {
    padding-right: 0.85rem;
  }

  .chip.more {
    gap: 0.3rem;
    padding: 0 0.65rem 0 0.85rem;
  }

  .chip:hover,
  .chip.more.open {
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

  .more-wrap {
    position: relative;
    flex-shrink: 0;
  }

  .menu {
    position: absolute;
    top: calc(100% + 0.4rem);
    left: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    width: 16rem;
    padding: 0.35rem;
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    background: var(--surface-2);
    box-shadow: var(--shadow);
    animation: menu-in 0.15s var(--ease-out);
  }

  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }

  .search {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    margin-bottom: 0.3rem;
    padding: 0 0.6rem;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text-muted);
  }

  .search input {
    flex: 1;
    min-width: 0;
    height: 2rem;
    border: none;
    outline: none;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 0.88rem;
  }

  .search:focus-within {
    border-color: var(--border-strong);
  }

  .items {
    display: flex;
    flex-direction: column;
    max-height: 18rem;
    overflow-y: auto;
  }

  .items button {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.55rem;
    padding: 0.5rem 0.6rem;
    border: none;
    border-radius: 8px;
    background: none;
    color: var(--text-muted);
    font-size: 0.88rem;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }

  .items button .name {
    flex: 1;
    max-width: none;
  }

  .items button:hover,
  .items button:focus-visible {
    background: var(--surface-3);
    color: var(--text);
    outline: none;
  }

  .none {
    padding: 0.5rem 0.6rem;
    color: var(--text-muted);
    font-size: 0.85rem;
  }
</style>
