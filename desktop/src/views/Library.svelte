<script lang="ts">
  import { api, errorMessage, on, type ClipView, type MyUpload, type Sort } from "../lib/api";
  import { bufferLabel, groupByDay } from "../lib/format";
  import { gameCounts, matchesPlace, mergeClips, visibleClips, type Place } from "../lib/myClips";
  import { toast } from "../lib/toasts.svelte";
  import ClipCard from "../components/ClipCard.svelte";
  import ClipPlayer from "../components/ClipPlayer.svelte";
  import GameFilter from "../components/GameFilter.svelte";
  import Icon from "../components/Icon.svelte";
  import Keys from "../components/Keys.svelte";
  import Modal from "../components/Modal.svelte";
  import SearchInput from "../components/SearchInput.svelte";
  import OnlineClipCard from "../components/OnlineClipCard.svelte";
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

  // My clips = the clips on this PC plus your uploads on the server (myClips.ts merges them).
  let local = $state<ClipView[]>([]);
  /** Your uploads; null when logged out (or not loaded yet). */
  let uploads = $state<MyUpload[] | null>(null);
  let uploadsError = $state<string | null>(null);
  let place = $state<Place>("all");
  let game = $state<string | null>(null);
  let sort = $state<Sort>("newestFirst");
  let search = $state("");
  let loaded = $state(false);
  let loadError = $state<string | null>(null);
  let now = $state(Date.now());

  let uploading = $state<ClipView | null>(null);
  let renaming = $state<{ kind: "local" | "online"; id: string; value: string } | null>(null);
  let tagging = $state<{ clip: ClipView; value: string } | null>(null);
  let removing = $state<{ clip: ClipView; deleteFile: boolean } | null>(null);
  let deletingOnline = $state<MyUpload | null>(null);
  let playing = $state<MyUpload | null>(null);
  let downloading = $state<Set<string>>(new Set());
  let saving = $state(false);

  const items = $derived(mergeClips(local, uploads ?? []));
  const games = $derived(gameCounts(items.filter((i) => matchesPlace(i, place))));
  const visible = $derived(visibleClips(items, place, game, sort, search));
  const groups = $derived(groupByDay(visible, (i) => i.at, now));

  $effect(() => {
    // A filter for a game with no clips left (in this place) would show nothing forever.
    if (game !== null && !games.some((g) => g.name === game)) game = null;
  });

  // Bumped to drop answers from older requests.
  let localRequest = 0;
  let uploadsRequest = 0;

  async function loadLocal() {
    const mine = ++localRequest;
    try {
      const list = await api.listClips({ game: null, sort: "newestFirst" });
      if (mine !== localRequest) return;
      local = list;
      loadError = null;
    } catch (e) {
      if (mine === localRequest) loadError = errorMessage(e);
    } finally {
      if (mine === localRequest) loaded = true;
    }
  }

  async function loadUploads() {
    const mine = ++uploadsRequest;
    try {
      const list = await api.myUploads();
      if (mine !== uploadsRequest) return;
      uploads = list;
      uploadsError = null;
    } catch (e) {
      if (mine === uploadsRequest) uploadsError = errorMessage(e);
    }
  }

  function load() {
    void loadLocal();
    void loadUploads();
  }

  $effect(() => {
    load();
    const timer = setInterval(() => (now = Date.now()), 30_000);
    const stops = [
      on("clip-saved", (clip) => {
        now = Date.now();
        local = [clip, ...local.filter((c) => c.id !== clip.id)];
      }),
      on("upload-done", load),
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

  async function toggleFavorite(clip: ClipView) {
    const favorite = !clip.favorite;
    // Shown at once; put back if the library refuses.
    local = local.map((c) => (c.id === clip.id ? { ...c, favorite } : c));
    if (!(await act(() => api.setClipFavorite(clip.id, favorite)))) void loadLocal();
  }

  async function copyLink(clip: ClipView) {
    await act(() => api.copyShareLink(clip.id), "Link copied!");
  }

  /** Puts the server's answer in place of the upload it updates. */
  function replaceUpload(updated: MyUpload) {
    if (uploads) uploads = uploads.map((u) => (u.id === updated.id ? updated : u));
  }

  async function submitRename(e: SubmitEvent) {
    e.preventDefault();
    if (!renaming || !renaming.value.trim()) return;
    const { kind, id, value } = renaming;
    const done =
      kind === "local"
        ? await act(() => api.renameClip(id, value))
        : await act(async () => replaceUpload(await api.updateUpload(id, { title: value })));
    if (done) {
      renaming = null;
      if (kind === "local") void loadLocal();
    }
  }

  async function submitGame(e: SubmitEvent) {
    e.preventDefault();
    if (!tagging) return;
    const name = tagging.value.trim() || null;
    if (await act(() => api.setClipGame(tagging!.clip.id, name))) {
      tagging = null;
      void loadLocal();
    }
  }

  async function confirmRemove() {
    if (!removing) return;
    const { clip, deleteFile } = removing;
    if (await act(() => api.deleteClip(clip.id, deleteFile), deleteFile ? "Clip deleted." : "Removed from library.")) {
      removing = null;
      void loadLocal();
    }
  }

  async function toggleVisibility(upload: MyUpload) {
    const visibility = upload.visibility === "feed" ? "link" : "feed";
    await act(
      async () => replaceUpload(await api.updateUpload(upload.id, { visibility })),
      visibility === "feed" ? "Now in the feed." : "Now link-only.",
    );
  }

  async function download(upload: MyUpload) {
    downloading = new Set(downloading).add(upload.id);
    if (await act(() => api.downloadUpload(upload.id), "Downloaded to your clips folder.")) await loadLocal();
    const rest = new Set(downloading);
    rest.delete(upload.id);
    downloading = rest;
  }

  async function confirmDeleteOnline() {
    if (!deletingOnline) return;
    const { id } = deletingOnline;
    if (await act(() => api.deleteUpload(id), "Deleted from the server.")) {
      deletingOnline = null;
      if (uploads) uploads = uploads.filter((u) => u.id !== id);
      void loadLocal();
    }
  }

  const PLACES: { value: Place; label: string }[] = [
    { value: "all", label: "All" },
    { value: "local", label: "On this PC" },
    { value: "uploaded", label: "Uploaded" },
    { value: "favorites", label: "Favorites" },
  ];
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

  <div class="filters">
    <div class="places" role="group" aria-label="Show clips">
      {#each PLACES as p (p.value)}
        <button class:active={place === p.value} aria-pressed={place === p.value} onclick={() => (place = p.value)}>
          {p.label}
        </button>
      {/each}
    </div>
    <SearchInput bind:value={search} placeholder="Search by title or game" label="Search my clips" />
    {#if uploadsError}
      <p class="uploads-error">
        <Icon name="alert" size={15} /> Couldn't load your uploads.
        <button class="link" onclick={loadUploads}>Try again</button>
      </p>
    {/if}
  </div>

  {#if games.length > 0}
    <GameFilter {games} selected={game} onselect={(g) => (game = g)} />
  {/if}

  {#if loadError}
    <div class="empty">
      <div class="empty-icon error"><Icon name="alert" size={28} /></div>
      <p>{loadError}</p>
      <button class="btn" onclick={load}><Icon name="retry" size={16} /> Try again</button>
    </div>
  {:else if loaded && visible.length === 0 && (game || place !== "all" || search.trim())}
    <div class="empty">
      <div class="empty-icon"><Icon name={place === "favorites" && !search.trim() ? "star" : "film"} size={28} /></div>
      {#if search.trim()}
        <p>No clips match “{search.trim()}”.</p>
      {:else if place === "favorites"}
        <p>No favorites yet. Star a clip to keep it here.</p>
      {:else if place === "uploaded" && uploads === null && !uploadsError}
        <p>Log in to see your uploads.</p>
      {:else if game}
        <p>No clips for this game here.</p>
      {:else if place === "uploaded"}
        <p>Nothing uploaded yet.</p>
      {:else}
        <p>No clips on this PC.</p>
      {/if}
    </div>
  {:else if loaded && visible.length === 0}
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
            {#each group.items as item, i (item.key)}
              {#if item.kind === "local"}
                {@const clip = item.clip}
                <ClipCard
                  {clip}
                  {now}
                  index={i}
                  onupload={() => (uploading = clip)}
                  onedit={onedit ? () => onedit(clip) : undefined}
                  onrename={() => (renaming = { kind: "local", id: clip.id, value: clip.title })}
                  onsetgame={() => (tagging = { clip, value: clip.gameName ?? "" })}
                  onremove={() => (removing = { clip, deleteFile: false })}
                  onfavorite={() => toggleFavorite(clip)}
                  onshowfolder={() => act(() => api.showInFolder(clip.id))}
                  oncopy={() => copyLink(clip)}
                />
              {:else}
                {@const upload = item.upload}
                <OnlineClipCard
                  {upload}
                  {now}
                  index={i}
                  downloading={downloading.has(upload.id)}
                  onplay={() => (playing = upload)}
                  oncopy={() => act(() => api.copyText(upload.shareUrl), "Link copied!")}
                  onopenweb={() => act(() => api.openWebsite(upload.shareUrl))}
                  onrename={() => (renaming = { kind: "online", id: upload.id, value: upload.title })}
                  onvisibility={() => toggleVisibility(upload)}
                  ondownload={() => download(upload)}
                  ondelete={() => (deletingOnline = upload)}
                />
              {/if}
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

{#if playing}
  <ClipPlayer clip={playing} {now} onclose={() => (playing = null)} />
{/if}

{#if deletingOnline}
  <Modal title="Delete from server?" onclose={() => (deletingOnline = null)}>
    <p>
      <strong>{deletingOnline.title}</strong> will be deleted from the server. Its link stops working for everyone.
      {#if local.some((c) => c.serverClipId === deletingOnline?.id && !c.fileMissing)}The copy on this PC stays.{/if}
    </p>
    {#snippet footer()}
      <button class="btn" onclick={() => (deletingOnline = null)}>Cancel</button>
      <button class="btn btn-danger" onclick={confirmDeleteOnline}>Delete</button>
    {/snippet}
  </Modal>
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
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem 1rem;
    margin-bottom: 0.75rem;
  }

  .places {
    display: inline-flex;
    gap: 0.2rem;
    padding: 0.2rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
  }

  .places button {
    padding: 0.35rem 0.9rem;
    border: none;
    border-radius: 999px;
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-size: 0.88rem;
    font-weight: 600;
    cursor: pointer;
  }

  .places button:hover {
    color: var(--text);
  }

  .places button.active {
    background: var(--surface-3);
    color: var(--text);
  }

  .uploads-error {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--warning);
    font-size: 0.86rem;
  }

  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent-text);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }

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
