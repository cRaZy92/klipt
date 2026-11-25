<script lang="ts">
  import { api, errorMessage, MAX_ALBUM_CLIPS, type AlbumSummary } from "../lib/api";
  import { toast } from "../lib/toasts.svelte";
  import Icon from "./Icon.svelte";

  /**
   * Picks albums for an upload to go into once it's online. `selected` holds their ids. `count`
   * is how many clips are going in, so an album without room for them all can't be picked.
   */
  let { selected = $bindable([]), count = 1 }: { selected?: string[]; count?: number } = $props();

  /** Null until loaded; stays null (and nothing shows) if they can't be loaded. */
  let albums = $state<AlbumSummary[] | null>(null);
  let naming = $state(false);
  let name = $state("");
  let creating = $state(false);

  $effect(() => {
    api
      .listAlbums()
      .then((list) => (albums = list))
      .catch(() => (albums = null));
  });

  const full = (a: AlbumSummary) => a.clipCount + count > MAX_ALBUM_CLIPS;

  function toggle(id: string) {
    selected = selected.includes(id) ? selected.filter((s) => s !== id) : [...selected, id];
  }

  async function create() {
    const title = name.trim();
    if (!title || creating) return;
    creating = true;
    try {
      const album = await api.createAlbum(title, []);
      albums = [album, ...(albums ?? [])];
      selected = [...selected, album.id];
      name = "";
      naming = false;
    } catch (e) {
      toast(`Couldn't make the album: ${errorMessage(e)}`, "error");
    } finally {
      creating = false;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    // Enter makes the album instead of submitting the upload form.
    if (e.key === "Enter") {
      e.preventDefault();
      void create();
    } else if (e.key === "Escape") {
      // Closes the name field, not the dialog.
      e.stopPropagation();
      naming = false;
      name = "";
    }
  }
</script>

{#if albums}
  <fieldset>
    <legend>Add to album <span class="optional">optional</span></legend>
    <div class="albums">
      {#each albums as album (album.id)}
        {@const on = selected.includes(album.id)}
        <button
          type="button"
          class="album"
          class:on
          aria-pressed={on}
          disabled={!on && full(album)}
          title={!on && full(album) ? `${album.title} is full (${MAX_ALBUM_CLIPS} clips)` : album.title}
          onclick={() => toggle(album.id)}
        >
          {#if on}<Icon name="check" size={13} />{/if}
          <span class="name">{album.title}</span>
        </button>
      {/each}
      {#if naming}
        <span class="new">
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="input"
            bind:value={name}
            maxlength="100"
            placeholder="Album name"
            aria-label="New album name"
            autofocus
            disabled={creating}
            {onkeydown}
          />
          <button type="button" class="btn btn-sm btn-primary" disabled={!name.trim() || creating} onclick={create}>
            Create
          </button>
        </span>
      {:else}
        <button type="button" class="album add" onclick={() => (naming = true)}>
          <Icon name="plus" size={13} /> New album
        </button>
      {/if}
    </div>
  </fieldset>
{/if}

<style>
  fieldset {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    margin-bottom: 0.4rem;
    color: var(--text-muted);
    font-size: 0.87rem;
    font-weight: 600;
  }

  .optional {
    margin-left: 0.3rem;
    color: var(--text-faint);
    font-weight: 500;
  }

  .albums {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
    max-height: 7.5rem;
    overflow-y: auto;
  }

  .album {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    max-width: 14rem;
    padding: 0.3rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-muted);
    font: inherit;
    font-size: 0.87rem;
    font-weight: 600;
    cursor: pointer;
    transition:
      border-color 0.15s,
      color 0.15s,
      background 0.15s;
  }

  .album:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .album:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .album.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }

  .album.add {
    border-style: dashed;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .new {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }

  .new .input {
    width: 11rem;
    padding-block: 0.3rem;
    font-size: 0.87rem;
  }
</style>
