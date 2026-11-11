<script lang="ts">
  import { api, errorMessage, on, type ClipView, type GameCount, type Sort } from "../lib/api";
  import { bufferLabel, groupByDay } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import ClipCard from "../components/ClipCard.svelte";
  import GameFilter from "../components/GameFilter.svelte";
  import Icon from "../components/Icon.svelte";
  import Keys from "../components/Keys.svelte";
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

  const groups = $derived(groupByDay(clips, (c) => c.createdAt, now));

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

<section class="page">
  <header class="page-head">
    <div>
      <h1>My clips</h1>
      {#if hotkey}
        <p class="subtitle">Press <Keys shortcut={hotkey} /> in a game to save the last {bufferLabel(bufferSeconds)}.</p>
      {/if}
    </div>
    <div class="page-tools">
      <button
        class="btn btn-ghost"
        onclick={() => (sort = sort === "newestFirst" ? "oldestFirst" : "newestFirst")}
        title="Change the order"
      >
        <Icon name="sort" size={16} />
        {sort === "newestFirst" ? "Newest first" : "Oldest first"}
      </button>
      <button class="btn btn-rec" onclick={saveNow} disabled={saving}>
        <span class="rec-dot"></span> Save clip now
      </button>
    </div>
  </header>

  {#if games.length > 0}
    <GameFilter {games} selected={game} onselect={(g) => (game = g)} />
  {/if}

  {#if loadError}
    <div class="empty">
      <div class="empty-icon error"><Icon name="alert" size={28} /></div>
      <p>{loadError}</p>
      <button class="btn" onclick={load}><Icon name="retry" size={16} /> Try again</button>
    </div>
  {:else if loaded && clips.length === 0}
    {#if game}
      <div class="empty">
        <div class="empty-icon"><Icon name="film" size={28} /></div>
        <p>No clips for this game.</p>
      </div>
    {:else}
      <div class="empty first-run">
        {#if hotkey}
          <div class="hero-keys"><Keys shortcut={hotkey} size="lg" /></div>
        {:else}
          <div class="empty-icon"><Icon name="film" size={28} /></div>
        {/if}
        <h2>Your best moments land here</h2>
        <p>
          Play something and press {#if hotkey}<strong>{hotkey}</strong>{:else}the hotkey{/if}. Klipt saves the last
          {bufferLabel(bufferSeconds)} as a clip.
        </p>
        <ol class="steps">
          <li><span>1</span> Play a game</li>
          <li><span>2</span> Press the hotkey</li>
          <li><span>3</span> Upload and share the link</li>
        </ol>
      </div>
    {/if}
  {:else}
    <div class="clip-groups">
      {#each groups as group (group.key)}
        <section aria-label={group.label}>
          <h2 class="group-label">{group.label}</h2>
          <div class="clip-grid">
            {#each group.items as clip, i (clip.id)}
              <ClipCard
                {clip}
                {now}
                index={i}
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
        </section>
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
  .first-run {
    gap: 0.9rem;
    padding-top: 3.5rem;
  }

  .first-run p {
    max-width: 28rem;
  }

  .first-run strong {
    color: var(--text);
  }

  .hero-keys {
    margin-bottom: 0.75rem;
    animation: float 4s ease-in-out infinite;
  }

  @keyframes float {
    50% {
      transform: translateY(-5px);
    }
  }

  .steps {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.6rem;
    margin: 0.75rem 0 0;
    padding: 0;
    list-style: none;
  }

  .steps li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.45rem 0.85rem 0.45rem 0.45rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text);
    font-size: 0.88rem;
    font-weight: 600;
  }

  .steps span {
    display: grid;
    place-items: center;
    width: 1.5rem;
    height: 1.5rem;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent-text);
    font-size: 0.78rem;
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
