<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { checkFolder, importing } from "../lib/imports.svelte";
  import ImportDialog from "../components/ImportDialog.svelte";
  import ImportPanel from "../components/ImportPanel.svelte";
  import { api, errorMessage, fileUrl, on, type ClipView, type MyUpload, type Sort } from "../lib/api";
  import { bufferLabel, groupByDay, uploadTitle } from "../lib/format";
  import { libraryMemory as memory } from "../lib/libraryMemory";
  import type { RemoveWhere } from "../lib/removal";
  import { keepShown, selectClick } from "../lib/selection";
  import { isBusy, uploadInTurn, uploads as uploadJobs } from "../lib/uploads.svelte";
  import { gameCounts, matchesPlace, mergeClips, visibleClips, type MediaKind, type Place } from "../lib/myClips";
  import { toast } from "../lib/toasts.svelte";
  import ClipCard from "../components/ClipCard.svelte";
  import ClipPlayer from "../components/ClipPlayer.svelte";
  import DeleteClipDialog from "../components/DeleteClipDialog.svelte";
  import GameFilter from "../components/GameFilter.svelte";
  import ImageViewer from "../components/ImageViewer.svelte";
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
  // Back from the editor (or another tab): start from what was shown, then refresh it.
  let local = $state<ClipView[]>(memory.local ?? []);
  /** Your uploads; null when logged out (or not loaded yet). */
  let uploads = $state<MyUpload[] | null>(memory.uploads);
  let uploadsError = $state<string | null>(null);
  let place = $state<Place>(memory.place);
  let game = $state<string | null>(memory.game);
  let sort = $state<Sort>(memory.sort);
  let search = $state(memory.search);
  let media = $state<MediaKind>(memory.media);
  /** The screenshot shown full size. */
  let viewing = $state<ClipView | null>(null);
  let loaded = $state(memory.local !== null);
  let page = $state<HTMLElement>();
  let loadError = $state<string | null>(null);
  let now = $state(Date.now());

  let uploading = $state<ClipView | null>(null);
  let renaming = $state<{ kind: "local" | "online"; id: string; value: string } | null>(null);
  /** Setting the game of one clip or several. */
  let tagging = $state<{ clips: ClipView[]; value: string; skipped: number } | null>(null);
  /** Deleting one clip or several: from the library only, also the files on this PC, or also the uploads. */
  let removing = $state<{
    local: { clip: ClipView; served: MyUpload | null }[];
    online: MyUpload[];
  } | null>(null);
  let deleting = $state(false);
  let bulkUpload = $state<{ clips: ClipView[]; visibility: "feed" | "link" } | null>(null);
  /** Cards picked for a bulk action, by `MyClip.key`; `anchor` is the last one clicked (for Shift). */
  let selected = $state<Set<string>>(new Set());
  let anchor: string | null = null;
  let deletingOnline = $state<MyUpload | null>(null);
  let playing = $state<MyUpload | null>(null);
  let downloading = $state<Set<string>>(new Set());
  let saving = $state(false);
  /** Bumped when a clip's thumbnail file is made again, so its picture reloads. */
  let thumbVersion = $state<Record<string, number>>({});

  const items = $derived(mergeClips(local, uploads ?? []));
  /** A local clip's uploaded copy, by server id: its card plays that copy and shows its compression. */
  const uploadById = $derived(new Map((uploads ?? []).map((u) => [u.id, u])));
  const games = $derived(gameCounts(items.filter((i) => matchesPlace(i, place))));
  const visible = $derived(visibleClips(items, place, game, sort, search, media));
  const groups = $derived(groupByDay(visible, (i) => i.at, now));
  /** The cards in the order they're shown, for Shift+click runs and "Select all". */
  const order = $derived(groups.flatMap((g) => g.items.map((i) => i.key)));
  const picked = $derived.by(() => {
    const chosen = visible.filter((i) => selected.has(i.key));
    return {
      local: chosen.flatMap((i) =>
        i.kind === "local" ? [{ clip: i.clip, served: (i.clip.serverClipId && uploadById.get(i.clip.serverClipId)) || null }] : [],
      ),
      online: chosen.flatMap((i) => (i.kind === "online" ? [i.upload] : [])),
    };
  });
  /** Selected clips that can be uploaded now: on this PC, not uploaded, not already uploading. */
  const uploadable = $derived(
    picked.local.map((p) => p.clip).filter((c) => !c.fileMissing && !c.shareUrl && !isBusy(uploadJobs[c.id])),
  );
  const allStarred = $derived(
    picked.local.every((p) => p.clip.favorite) && picked.online.every((u) => u.favorite !== false),
  );

  $effect(() => {
    // Filtered out or deleted: no longer selected (actions only touch what you can see).
    const kept = keepShown(selected, order);
    if (kept !== selected) selected = kept;
  });

  function toggleSelect(key: string, e: MouseEvent) {
    selected = selectClick(selected, order, key, anchor, e.shiftKey);
    anchor = key;
  }

  function clearSelection() {
    selected = new Set();
    anchor = null;
  }

  const dialogOpen = $derived(
    !!(uploading || playing || viewing || renaming || tagging || removing || deletingOnline || bulkUpload),
  );

  function onkeydown(e: KeyboardEvent) {
    if (dialogOpen) return;
    const typing = (e.target as HTMLElement | null)?.closest("input, textarea, select, [contenteditable]");
    if (e.key === "Escape" && selected.size > 0) {
      clearSelection();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a" && !typing) {
      e.preventDefault();
      selected = new Set(order);
    }
  }

  $effect(() => {
    // A filter for a game with no clips left (in this place) would show nothing forever.
    if (game !== null && !games.some((g) => g.name === game)) game = null;
  });

  $effect(() => {
    Object.assign(memory, { place, game, sort, search, media });
  });

  $effect(() => {
    memory.local = $state.snapshot(local);
    memory.uploads = $state.snapshot(uploads);
  });

  // The app scrolls its <main>, not this page: put it back where it was, then keep track of it.
  $effect(() => {
    const scroller = page?.closest("main");
    if (!scroller) return;
    if (memory.local !== null) scroller.scrollTop = memory.scrollTop;
    const onscroll = () => (memory.scrollTop = scroller.scrollTop);
    scroller.addEventListener("scroll", onscroll, { passive: true });
    return () => scroller.removeEventListener("scroll", onscroll);
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
      // Linked local clips may have changed: unlinked (deleted elsewhere) or edited elsewhere.
      // Also picks up edits made on the website, copied into linked local clips by myUploads.
      if (list && local.some((c) => c.serverClipId)) void loadLocal();
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
      on("clip-imported", (clip) => {
        local = [clip, ...local.filter((c) => c.id !== clip.id)];
      }),
      on("upload-done", load),
      on("thumbnail-changed", ({ id }) => (thumbVersion = { ...thumbVersion, [id]: Date.now() })),
      on("clips-cleaned", ({ deleted }) => {
        void loadLocal();
        toast(`Cleanup deleted ${deleted === 1 ? "1 old clip" : `${deleted} old clips`}.`, "info");
      }),
    ];
    return () => {
      clearInterval(timer);
      stops.forEach((stop) => stop());
    };
  });

  // While the server compresses one of your uploads, check back so its "Queued" / "Processing"
  // pill goes away once it's done.
  $effect(() => {
    if (!uploads?.some((u) => u.processing === "queued" || u.processing === "running")) return;
    const timer = setInterval(() => void loadUploads(), 15_000);
    return () => clearInterval(timer);
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

  /** Shows a server clip as starred or not in the uploads list. */
  function markUpload(id: string, favorite: boolean) {
    if (uploads) uploads = uploads.map((u) => (u.id === id ? { ...u, favorite } : u));
  }

  /** A clip on this PC: starred locally (the cleanup keeps it) and, once uploaded, on the server too. */
  async function toggleFavorite(clip: ClipView, favorite = !clip.favorite) {
    const serverId = clip.serverClipId && uploads?.some((u) => u.id === clip.serverClipId) ? clip.serverClipId : null;
    // Shown at once; put back if the library or the server refuses.
    local = local.map((c) => (c.id === clip.id ? { ...c, favorite } : c));
    if (serverId) markUpload(serverId, favorite);
    // The library sends the star to the server copy itself (and again later if it's offline).
    const ok = await act(() => api.setClipFavorite(clip.id, favorite));
    if (!ok) load();
  }

  /** An upload with no copy on this PC: starred on the server. */
  async function toggleOnlineFavorite(upload: MyUpload, favorite = !upload.favorite) {
    markUpload(upload.id, favorite);
    if (!(await act(() => api.setServerFavorite(upload.id, favorite)))) markUpload(upload.id, !favorite);
  }

  /** Stars every selected clip, or unstars them when they all are starred already. */
  async function starSelected() {
    const favorite = !allStarred;
    await Promise.all([
      ...picked.local.filter((p) => p.clip.favorite !== favorite).map((p) => toggleFavorite(p.clip, favorite)),
      ...picked.online
        .filter((u) => u.favorite !== undefined && u.favorite !== favorite)
        .map((u) => toggleOnlineFavorite(u, favorite)),
    ]);
  }

  async function startBulkUpload() {
    if (!bulkUpload) return;
    const { clips, visibility } = bulkUpload;
    bulkUpload = null;
    clearSelection();
    const done = await uploadInTurn(
      clips.map((c) => ({ id: c.id, title: uploadTitle(c) })),
      visibility,
    );
    if (done > 0) toast(`Uploaded ${done === 1 ? "1 clip" : `${done} clips`}. Copy a link from its card to share it.`, "success");
  }

  /** Picks a folder (ShadowPlay, OBS, Medal…) and imports the clips in it. */
  async function importClips() {
    const folder = await openDialog({ directory: true, title: "Choose a folder with clips to import" });
    if (typeof folder === "string") await checkFolder(folder);
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
    const ids = tagging.clips.map((c) => c.id);
    if (await act(async () => { for (const id of ids) await api.setClipGame(id, name); })) {
      tagging = null;
      void loadLocal();
    } else {
      void loadLocal();
    }
  }

  async function confirmRemove(where: RemoveWhere) {
    if (!removing) return;
    const { local: clips, online } = removing;
    deleting = true;
    const total = clips.length + online.length;
    let failed = 0;
    let firstError = "";
    const attempt = async (work: () => Promise<unknown>) => {
      try {
        await work();
      } catch (e) {
        failed++;
        firstError ||= errorMessage(e);
      }
    };
    for (const { clip, served } of clips) {
      await attempt(async () => {
        // The server first: if it refuses, the clip is still whole on this PC.
        if (where === "everywhere" && served) await api.deleteUpload(served.id);
        await api.deleteClip(clip.id, where !== "library" && !clip.fileMissing);
      });
    }
    if (where === "everywhere") for (const upload of online) await attempt(() => api.deleteUpload(upload.id));
    const done = where === "everywhere" ? total : clips.length;
    if (failed === 0) {
      toast(
        total === 1
          ? where === "everywhere" ? "Deleted from this PC and the server." : where === "local" ? "Clip deleted." : "Removed from library."
          : `${where === "library" ? "Removed" : "Deleted"} ${done} clips.`,
        "success",
      );
      removing = null;
      clearSelection();
    } else {
      toast(total === 1 ? firstError : `${failed} of ${done} couldn't be deleted: ${firstError}`, "error");
      removing = null;
    }
    deleting = false;
    load();
  }

  /** Opens the delete dialog for these clips. */
  function askRemove(local: { clip: ClipView; served: MyUpload | null }[], online: MyUpload[]) {
    removing = { local, online };
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

  const MEDIA: { value: MediaKind; label: string }[] = [
    { value: "all", label: "Everything" },
    { value: "clip", label: "Clips" },
    { value: "screenshot", label: "Screenshots" },
  ];
</script>

<section class="page" bind:this={page}>
  <div class="page-top">
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
        <button
          class="btn btn-ghost"
          onclick={importClips}
          disabled={importing.phase !== "idle"}
          title="Add clips you already have (ShadowPlay, OBS, Medal…) from a folder"
        >
          <Icon name="folder" size={16} /> Import…
        </button>
        <button class="btn" onclick={() => act(() => api.takeScreenshot())} title="Take a screenshot of the recorded screen">
          <Icon name="image" size={16} /> Screenshot
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
      <div class="places" role="group" aria-label="Clips or screenshots">
        {#each MEDIA as m (m.value)}
          <button class:active={media === m.value} aria-pressed={media === m.value} onclick={() => (media = m.value)}>
            {m.label}
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
  </div>

  <ImportPanel />

  {#if loadError}
    <div class="empty">
      <div class="empty-icon error"><Icon name="alert" size={28} /></div>
      <p>{loadError}</p>
      <button class="btn" onclick={load}><Icon name="retry" size={16} /> Try again</button>
    </div>
  {:else if loaded && visible.length === 0 && (game || place !== "all" || search.trim() || media !== "all")}
    <div class="empty">
      <div class="empty-icon"><Icon name={place === "favorites" && !search.trim() ? "star" : "film"} size={28} /></div>
      {#if search.trim()}
        <p>No clips match “{search.trim()}”.</p>
      {:else if media === "screenshot"}
        <p>No screenshots here yet. Press your screenshot shortcut in a game, or use the Screenshot button.</p>
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
        <p class="import-hint">
          Already have clips from ShadowPlay, OBS or Medal?
          <button class="link" onclick={importClips} disabled={importing.phase !== "idle"}>Import them from a folder</button>
        </p>
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
                {@const served = clip.serverClipId ? uploadById.get(clip.serverClipId) : undefined}
                <ClipCard
                  {clip}
                  {now}
                  index={i}
                  onupload={() => (uploading = clip)}
                  onedit={clip.kind === "screenshot" ? undefined : onedit ? () => onedit(clip) : undefined}
                  onview={clip.kind === "screenshot" ? () => (viewing = clip) : undefined}
                  onplay={served ? () => (playing = served) : undefined}
                  processing={served?.processing}
                  onrename={() => (renaming = { kind: "local", id: clip.id, value: clip.title })}
                  onsetgame={() => (tagging = { clips: [clip], value: clip.gameName ?? "", skipped: 0 })}
                  onremove={() => askRemove([{ clip, served: served ?? null }], [])}
                  selected={selected.has(item.key)}
                  selecting={selected.size > 0}
                  onselect={(e) => toggleSelect(item.key, e)}
                  onfavorite={() => toggleFavorite(clip)}
                  thumbVersion={thumbVersion[clip.id] ?? clip.thumbnailMs ?? 0}
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
                  onfavorite={upload.favorite === undefined ? undefined : () => toggleOnlineFavorite(upload)}
                  selected={selected.has(item.key)}
                  selecting={selected.size > 0}
                  onselect={(e) => toggleSelect(item.key, e)}
                />
              {/if}
            {/each}
          </div>
        </section>
      {/each}
    </div>
  {/if}

  {#if selected.size > 0}
    <div class="bulk-bar" role="toolbar" aria-label="Selected clips">
      <span class="bulk-count">{selected.size} selected</span>
      {#if selected.size < order.length}
        <button class="btn btn-sm btn-ghost" onclick={() => (selected = new Set(order))} title="Ctrl+A">
          Select all {order.length}
        </button>
      {/if}
      <span class="bulk-sep" aria-hidden="true"></span>
      <button class="btn btn-sm btn-ghost" onclick={starSelected}>
        <Icon name={allStarred ? "star" : "starFilled"} size={15} />
        {allStarred ? "Unstar" : "Star"}
      </button>
      <button
        class="btn btn-sm btn-ghost"
        disabled={picked.local.length === 0}
        title={picked.local.length === 0 ? "Only clips on this PC have a game you can change" : undefined}
        onclick={() =>
          (tagging = {
            clips: picked.local.map((p) => p.clip),
            value: picked.local.every((p) => p.clip.gameName === picked.local[0].clip.gameName)
              ? (picked.local[0].clip.gameName ?? "")
              : "",
            skipped: picked.online.length,
          })}
      >
        <Icon name="tag" size={15} /> Set game
      </button>
      <button
        class="btn btn-sm btn-ghost"
        disabled={uploadable.length === 0}
        title={uploadable.length === 0 ? "Nothing here left to upload" : undefined}
        onclick={() => (bulkUpload = { clips: uploadable, visibility: "feed" })}
      >
        <Icon name="upload" size={15} /> Upload{uploadable.length > 0 && uploadable.length < selected.size
          ? ` ${uploadable.length}`
          : ""}
      </button>
      <button class="btn btn-sm btn-ghost bulk-danger" onclick={() => askRemove(picked.local, picked.online)}>
        <Icon name="trash" size={15} /> Delete
      </button>
      <button class="btn btn-sm btn-ghost btn-icon" onclick={clearSelection} aria-label="Clear selection" title="Clear selection (Esc)">
        <Icon name="close" size={16} />
      </button>
    </div>
  {/if}
</section>

<svelte:window {onkeydown} />

{#if bulkUpload}
  {@const b = bulkUpload}
  <Modal title="Upload {b.clips.length === 1 ? '1 clip' : `${b.clips.length} clips`}" onclose={() => (bulkUpload = null)}>
    <p class="muted">
      They upload one after another, each with its own title (like “{uploadTitle(b.clips[0])}”). Links aren't copied; use
      “Copy link” on a card when it's done.
    </p>
    <div class="pick-list" role="radiogroup" aria-label="Who can see them">
      <label class="pick">
        <input type="radio" bind:group={bulkUpload.visibility} value="feed" />
        <span><strong>Feed</strong><small>Your friends see them in the feed.</small></span>
      </label>
      <label class="pick">
        <input type="radio" bind:group={bulkUpload.visibility} value="link" />
        <span><strong>Link only</strong><small>Only people you send a link to can watch them.</small></span>
      </label>
    </div>
    {#snippet footer()}
      <button class="btn" onclick={() => (bulkUpload = null)}>Cancel</button>
      <button class="btn btn-primary" onclick={startBulkUpload}><Icon name="upload" size={16} /> Upload</button>
    {/snippet}
  </Modal>
{/if}

{#if uploading}
  <UploadDialog clip={uploading} onclose={() => (uploading = null)} />
{/if}

{#if playing}
  <ClipPlayer clip={playing} {now} own onclose={() => (playing = null)} />
{/if}

{#if viewing}
  <ImageViewer src={fileUrl(viewing.path)} title={viewing.title} onclose={() => (viewing = null)} />
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
  <Modal
    title={tagging.clips.length === 1 ? "Set game" : `Set game of ${tagging.clips.length} clips`}
    onclose={() => (tagging = null)}
  >
    <form id="game-form" onsubmit={submitGame}>
      <label class="field">
        <span>Game</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={tagging.value} list="known-games" placeholder="Unknown game" autofocus />
        <datalist id="known-games">
          {#each games as g (g.name)}<option value={g.name}></option>{/each}
        </datalist>
      </label>
      {#if tagging.clips.length === 1 && !tagging.clips[0].gameName && tagging.clips[0].exe && tagging.clips[0].steamAppId === null}
        <p class="muted hint">Klipt will remember this game for future clips too.</p>
      {/if}
      {#if tagging.skipped > 0}
        <p class="muted hint">
          {tagging.skipped === 1 ? "1 selected clip is" : `${tagging.skipped} selected clips are`} only online and keep{tagging.skipped === 1
            ? "s"
            : ""} its game.
        </p>
      {/if}
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (tagging = null)}>Cancel</button>
      <button class="btn btn-primary" type="submit" form="game-form">Save</button>
    {/snippet}
  </Modal>
{/if}

<ImportDialog />

{#if removing}
  <DeleteClipDialog
    local={removing.local.map((p) => ({ title: p.clip.title, fileMissing: p.clip.fileMissing, uploaded: !!p.served }))}
    online={removing.online}
    busy={deleting}
    onconfirm={confirmRemove}
    onclose={() => (removing = null)}
  />
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

  .import-hint {
    margin-top: 1rem;
    color: var(--text-muted);
    font-size: 0.9rem;
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

  .bulk-bar {
    position: sticky;
    bottom: 1.25rem;
    z-index: 6;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    align-self: center;
    gap: 0.25rem;
    max-width: 100%;
    padding: 0.4rem 0.45rem 0.4rem 1rem;
    border: 1px solid var(--border-strong);
    border-radius: 14px;
    background: color-mix(in srgb, var(--surface-2) 92%, transparent);
    backdrop-filter: blur(12px);
    box-shadow: var(--shadow);
    animation: bar-in 0.18s var(--ease-out);
  }

  @keyframes bar-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }

  .bulk-count {
    margin-right: 0.35rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .bulk-sep {
    width: 1px;
    height: 1.4rem;
    margin: 0 0.3rem;
    background: var(--border-strong);
  }

  .bulk-danger {
    color: var(--danger-text, var(--danger));
  }

  .bulk-danger:hover:not(:disabled) {
    background: var(--danger-soft);
  }

</style>
