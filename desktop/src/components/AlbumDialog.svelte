<script lang="ts">
  import { api, errorMessage, type AlbumSummary } from "../lib/api";
  import { toast } from "../lib/toasts.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  let {
    clipIds,
    skipped = 0,
    onclose,
  }: {
    /** The server clips to put in an album (uploaded ones). */
    clipIds: string[];
    /** Picked clips left out because they aren't uploaded. */
    skipped?: number;
    onclose: () => void;
  } = $props();

  /** One clip: a check box per album (in it or not). Several: an Add button per album. */
  const single = $derived(clipIds.length === 1);
  let albums = $state<AlbumSummary[] | null>(null);
  let loadError = $state<string | null>(null);
  /** Album ids a change is being sent for. */
  let busy = $state<Set<string>>(new Set());
  let title = $state("");
  let creating = $state(false);

  async function load() {
    loadError = null;
    try {
      albums = await api.listAlbums(single ? clipIds[0] : undefined);
    } catch (e) {
      loadError = errorMessage(e);
    }
  }
  $effect(() => {
    void load();
  });

  const clipsLabel = (n: number) => (n === 1 ? "1 clip" : `${n} clips`);

  async function run(album: AlbumSummary, work: () => Promise<void>) {
    busy = new Set([...busy, album.id]);
    try {
      await work();
    } catch (e) {
      toast(`Couldn't change ${album.title}: ${errorMessage(e)}`, "error");
    } finally {
      busy = new Set([...busy].filter((id) => id !== album.id));
    }
  }

  /** One clip: in or out of `album`. */
  function toggle(album: AlbumSummary, add: boolean) {
    void run(album, async () => {
      await api.setAlbumClip(album.id, clipIds[0], add);
      album.hasClip = add;
      album.clipCount += add ? 1 : -1;
      toast(add ? `Added to ${album.title}` : `Removed from ${album.title}`);
    });
  }

  /** Several clips: all go into `album` (ones already in it stay where they are). */
  function addAll(album: AlbumSummary) {
    void run(album, async () => {
      for (const id of clipIds) await api.setAlbumClip(album.id, id, true);
      await load();
      toast(`Added ${clipsLabel(clipIds.length)} to ${album.title}`);
    });
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    const name = title.trim();
    if (!name || creating) return;
    creating = true;
    try {
      await api.createAlbum(name, clipIds);
      title = "";
      await load();
      toast(`Added to ${name}`);
    } catch (err) {
      toast(`Couldn't make the album: ${errorMessage(err)}`, "error");
    } finally {
      creating = false;
    }
  }
</script>

<Modal title={single ? "Add to album" : `Add ${clipsLabel(clipIds.length)} to an album`} {onclose} width="26rem">
  {#if skipped > 0}
    <p class="muted note">
      {skipped === 1 ? "1 picked clip isn't" : `${skipped} picked clips aren't`} uploaded, so {skipped === 1 ? "it's" : "they're"}
      left out: albums hold clips on the server.
    </p>
  {/if}
  {#if loadError}
    <p class="error">Couldn't load your albums: {loadError}</p>
    <button class="btn btn-sm" onclick={load}><Icon name="retry" size={15} /> Try again</button>
  {:else if albums === null}
    <p class="muted"><span class="spinner" aria-hidden="true"></span> Loading your albums…</p>
  {:else if albums.length === 0}
    <p class="muted">No albums yet. Name one below; anyone with its link can watch it.</p>
  {:else}
    <ul class="albums">
      {#each albums as album (album.id)}
        <li>
          {#if single}
            <label class="row">
              <input
                type="checkbox"
                checked={!!album.hasClip}
                disabled={busy.has(album.id)}
                onchange={(e) => toggle(album, e.currentTarget.checked)}
              />
              <span class="name" title={album.title}>{album.title}</span>
              <span class="count">{clipsLabel(album.clipCount)}</span>
            </label>
          {:else}
            <div class="row">
              <span class="name" title={album.title}>{album.title}</span>
              <span class="count">{clipsLabel(album.clipCount)}</span>
              <button class="btn btn-sm" disabled={busy.has(album.id)} onclick={() => addAll(album)}>Add</button>
            </div>
          {/if}
          <button
            class="btn btn-sm btn-ghost btn-icon"
            aria-label="Open {album.title} on the website"
            title="Open on the website"
            onclick={() => api.openWebsite(album.shareUrl).catch((e) => toast(errorMessage(e), "error"))}
          >
            <Icon name="external" size={15} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  <form class="new" onsubmit={create}>
    <input class="input" bind:value={title} maxlength="100" placeholder="New album" aria-label="New album name" />
    <button class="btn btn-primary" type="submit" disabled={!title.trim() || creating}>
      {single ? "Create and add" : `Create with ${clipsLabel(clipIds.length)}`}
    </button>
  </form>
</Modal>

<style>
  .note {
    margin: 0 0 0.75rem;
    font-size: 0.85rem;
  }

  .albums {
    display: grid;
    gap: 0.15rem;
    max-height: 50vh;
    margin: 0 -0.5rem;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .albums li {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding-right: 0.25rem;
    border-radius: var(--radius-sm);
  }

  .albums li:hover {
    background: var(--surface-3);
  }

  .row {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 0.65rem;
    min-width: 0;
    padding: 0.55rem 0.5rem;
  }

  label.row {
    cursor: pointer;
  }

  .row input {
    flex: none;
    width: 1.05rem;
    height: 1.05rem;
    accent-color: var(--accent);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    flex: none;
    color: var(--text-faint);
    font-size: 0.82rem;
  }

  .new {
    display: flex;
    gap: 0.5rem;
    margin-top: 1rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border);
  }

  .new .input {
    flex: 1;
    min-width: 0;
  }

  .error {
    color: var(--danger-text);
  }
</style>
