<script lang="ts">
  import { api, errorMessage, on, type ClipView, type GameCount, type Sort } from "../lib/api";
  import { bufferLabel } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import ClipCard from "../components/ClipCard.svelte";
  import GameFilter from "../components/GameFilter.svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import UploadDialog from "../components/UploadDialog.svelte";

  let {
    hotkey,
    bufferSeconds,
    onedit,
  }: {
    /** The save shortcut as the user should press it, e.g. "Alt + `". */
    hotkey: string;
    bufferSeconds: number;
    /** Opens the editor (Task 11); without it cards don't offer Edit. */
    onedit?: (clip: ClipView) => void;
  } = $props();

  let clips = $state<ClipView[]>([]);
  let games = $state<GameCount[]>([]);
  let game = $state<string | null>(null);
  let sort = $state<Sort>("newestFirst");
  let loaded = $state(false);
  let loadError = $state<string | null>(null);
  let now = $state(Date.now());

  let uploading = $state<ClipView | null>(null);
  let renaming = $state<{ clip: ClipView; value: string } | null>(null);
  let tagging = $state<{ clip: ClipView; value: string } | null>(null);
  let removing = $state<{ clip: ClipView; deleteFile: boolean } | null>(null);
  let saving = $state(false);

  // Bumped to drop answers from older requests when the filter changes quickly.
  let request = 0;

  async function load() {
    const mine = ++request;
    try {
      const [list, gameList] = await Promise.all([api.listClips({ game, sort }), api.listGames()]);
      if (mine !== request) return;
      clips = list;
      games = gameList;
      loadError = null;
      // A filter for a game whose last clip was removed would show nothing forever.
      if (game !== null && !gameList.some((g) => g.name === game)) game = null;
    } catch (e) {
      if (mine === request) loadError = errorMessage(e);
    } finally {
      if (mine === request) loaded = true;
    }
  }

  $effect(() => {
    // Reload whenever the filter or sort changes.
    void game;
    void sort;
    void load();
  });

  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 30_000);
    const stops = [
      on("clip-saved", (clip) => {
        now = Date.now();
        if (game === null && sort === "newestFirst" && !clips.some((c) => c.id === clip.id)) {
          clips = [clip, ...clips];
          void api.listGames().then((g) => (games = g)).catch(() => {});
        } else {
          void load();
        }
      }),
      on("upload-done", () => void load()),
    ];
    return () => {
      clearInterval(timer);
      stops.forEach((stop) => stop());
    };
  });

  async function act(action: () => Promise<unknown>, success?: string) {
    try {
      await action();
      if (success) toast(success, "success");
      return true;
    } catch (e) {
      toast(errorMessage(e), "error");
      return false;
    }
  }

  async function saveNow() {
    saving = true;
    await act(() => api.saveClipNow());
    // The clip arrives through `clip-saved`; this only debounces the button.
    setTimeout(() => (saving = false), 1500);
  }

  async function copyLink(clip: ClipView) {
    await act(() => api.copyShareLink(clip.id), "Link copied!");
  }

  async function submitRename(e: SubmitEvent) {
    e.preventDefault();
    if (!renaming || !renaming.value.trim()) return;
    if (await act(() => api.renameClip(renaming!.clip.id, renaming!.value))) {
      renaming = null;
      void load();
    }
  }

  async function submitGame(e: SubmitEvent) {
    e.preventDefault();
    if (!tagging) return;
    const name = tagging.value.trim() || null;
    if (await act(() => api.setClipGame(tagging!.clip.id, name))) {
      tagging = null;
      void load();
    }
  }

  async function confirmRemove() {
    if (!removing) return;
    const { clip, deleteFile } = removing;
    if (await act(() => api.deleteClip(clip.id, deleteFile), deleteFile ? "Clip deleted." : "Removed from library.")) {
      removing = null;
      void load();
    }
  }
</script>

<section class="library">
  <div class="toolbar">
    <div class="heading">
      <h1>Clips</h1>
      {#if loaded && clips.length > 0}<span class="count">{clips.length}</span>{/if}
    </div>
    <div class="tools">
      <button
        class="btn btn-ghost"
        onclick={() => (sort = sort === "newestFirst" ? "oldestFirst" : "newestFirst")}
        title="Change the order"
      >
        <Icon name="sort" size={16} />
        {sort === "newestFirst" ? "Newest first" : "Oldest first"}
      </button>
      <button class="btn" onclick={saveNow} disabled={saving}>
        <Icon name="record" size={16} /> Save clip now
      </button>
    </div>
  </div>

  {#if games.length > 0}
    <GameFilter {games} selected={game} onselect={(g) => (game = g)} />
  {/if}

  {#if loadError}
    <div class="empty">
      <Icon name="alert" size={32} />
      <p>{loadError}</p>
      <button class="btn" onclick={load}><Icon name="retry" size={16} /> Try again</button>
    </div>
  {:else if loaded && clips.length === 0}
    <div class="empty">
      <div class="empty-icon"><Icon name="film" size={30} /></div>
      {#if game}
        <p>No clips for this game.</p>
      {:else}
        <h2>No clips yet</h2>
        <p>
          Press <kbd>{hotkey}</kbd> in a game to save the last {bufferLabel(bufferSeconds)}.
        </p>
      {/if}
    </div>
  {:else}
    <div class="grid">
      {#each clips as clip (clip.id)}
        <ClipCard
          {clip}
          {now}
          onupload={() => (uploading = clip)}
          onedit={onedit ? () => onedit(clip) : undefined}
          onrename={() => (renaming = { clip, value: clip.title })}
          onsetgame={() => (tagging = { clip, value: clip.gameName ?? "" })}
          onremove={() => (removing = { clip, deleteFile: false })}
          onshowfolder={() => act(() => api.showInFolder(clip.id))}
          oncopy={() => copyLink(clip)}
        />
      {/each}
    </div>
  {/if}
</section>

{#if uploading}
  <UploadDialog clip={uploading} onclose={() => (uploading = null)} />
{/if}

{#if renaming}
  <Modal title="Rename clip" onclose={() => (renaming = null)}>
    <form id="rename-form" onsubmit={submitRename}>
      <label class="field">
        <span>Title</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={renaming.value} maxlength="100" autofocus />
      </label>
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (renaming = null)}>Cancel</button>
      <button class="btn btn-primary" type="submit" form="rename-form" disabled={!renaming?.value.trim()}>Save</button>
    {/snippet}
  </Modal>
{/if}

{#if tagging}
  <Modal title="Set game" onclose={() => (tagging = null)}>
    <form id="game-form" onsubmit={submitGame}>
      <label class="field">
        <span>Game</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={tagging.value} list="known-games" placeholder="Unknown game" autofocus />
        <datalist id="known-games">
          {#each games as g (g.name)}<option value={g.name}></option>{/each}
        </datalist>
      </label>
      {#if !tagging.clip.gameName && tagging.clip.exe && tagging.clip.steamAppId === null}
        <p class="muted hint">Klipt will remember this game for future clips too.</p>
      {/if}
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (tagging = null)}>Cancel</button>
      <button class="btn btn-primary" type="submit" form="game-form">Save</button>
    {/snippet}
  </Modal>
{/if}

{#if removing}
  <Modal title="Remove clip?" onclose={() => (removing = null)}>
    <p>
      <strong>{removing.clip.title}</strong> will be removed from your library.
      {#if removing.clip.shareUrl}It stays online on the server.{/if}
    </p>
    {#if !removing.clip.fileMissing}
      <label class="check">
        <input type="checkbox" bind:checked={removing.deleteFile} />
        Also delete the video file from this computer
      </label>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => (removing = null)}>Cancel</button>
      <button class="btn btn-danger" onclick={confirmRemove}>
        {removing?.deleteFile ? "Delete" : "Remove"}
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  .library {
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
    padding: 1.5rem 1.75rem 2.5rem;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .heading {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
  }

  h1 {
    font-size: 1.5rem;
    letter-spacing: -0.02em;
  }

  .count {
    color: var(--text-faint);
    font-weight: 600;
  }

  .tools {
    display: flex;
    gap: 0.5rem;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 1rem;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    padding: 5rem 1rem;
    color: var(--text-muted);
    text-align: center;
  }

  .empty-icon {
    display: grid;
    place-items: center;
    width: 4rem;
    height: 4rem;
    border-radius: 18px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .empty h2 {
    color: var(--text);
    font-size: 1.2rem;
  }

  kbd {
    padding: 0.1rem 0.45rem;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-weight: 600;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .hint {
    font-size: 0.85rem;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    cursor: pointer;
  }

  .check input {
    width: 1rem;
    height: 1rem;
    accent-color: var(--danger);
  }
</style>
