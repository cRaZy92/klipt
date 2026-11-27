<script lang="ts">
  import { api, fileUrl, type ClipView, type Visibility } from "../lib/api";
  import { toVisibility, VISIBILITY } from "../lib/visibility";
  import { editedLength } from "../lib/cuts";
  import { WHERE } from "../lib/myClips";
  import { formatDuration, hueOf, relativeDate } from "../lib/format";
  import { forgetUpload, fraction, isBusy, retryUpload, statusLabel, transferLabel, uploads } from "../lib/uploads.svelte";
  import { menu } from "../lib/a11y";
  import { releaseVideo } from "../lib/releaseVideo";
  import Icon from "./Icon.svelte";
  import ProcessingPill from "./ProcessingPill.svelte";
  import "./clip-card.css";

  let {
    clip,
    now,
    index = 0,
    onupload,
    onedit,
    onrename,
    onsetgame,
    onremove,
    onshowfolder,
    oncopy,
    onvisibility,
    onfavorite,
    onalbum,
    onview,
    onplay,
    processing = null,
    thumbVersion = 0,
    selected = false,
    selecting = false,
    onselect,
  }: {
    clip: ClipView;
    /** The time relative dates are counted from (ticks once a minute). */
    now: number;
    /** Position in its group, to stagger the cards' entrance. */
    index?: number;
    onupload: () => void;
    /** The editor (Task 11). The Edit action is hidden until it's wired. */
    onedit?: () => void;
    onrename: () => void;
    onsetgame: () => void;
    onremove: () => void;
    onshowfolder: () => void;
    oncopy: () => void;
    /** Uploaded: changes who finds the upload (Link only / Friends / Public), in place (same link). */
    onvisibility?: (v: Visibility) => void;
    /** Stars or unstars the clip; without it there's no star. */
    onfavorite?: () => void;
    /** Uploaded: "Add to album". */
    onalbum?: () => void;
    /** Changes when the thumbnail file is rewritten, so the cached picture isn't shown. */
    thumbVersion?: number;
    /** Shows a screenshot full size (screenshots have no editor). */
    onview?: () => void;
    /** Plays the uploaded copy (what friends see); once a clip is uploaded, the picture plays it. */
    onplay?: () => void;
    /** The server is still compressing the uploaded copy. */
    processing?: "queued" | "running" | null;
    /** Picked for a bulk action. */
    selected?: boolean;
    /** Something is selected: clicking the picture selects instead of opening. */
    selecting?: boolean;
    /** Toggles the selection (the event says whether Shift was held). */
    onselect?: (e: MouseEvent) => void;
  } = $props();

  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();
  let thumbFailed = $state(false);

  const job = $derived(uploads[clip.id]);
  const busy = $derived(isBusy(job));
  const uploaded = $derived(!!clip.shareUrl);
  const thumb = $derived(
    !clip.fileMissing && clip.thumbPath && !thumbFailed
      ? `${fileUrl(clip.thumbPath)}${thumbVersion ? `?v=${thumbVersion}` : ""}`
      : null,
  );
  const progress = $derived(job ? fraction(job) : null);
  /** What the edit leaves (what uploads and exports get); shown when it differs from the recording. */
  const length = $derived(formatDuration(editedLength(clip, clip.durationMs)));
  const recorded = $derived(formatDuration(clip.durationMs));

  // Hover preview: resting on the picture plays the clip muted (its kept part, looping), to find
  // "the one where…" without opening it. Not for screenshots, missing files, or while selecting.
  const HOVER_DELAY_MS = 450;
  let previewUrl = $state<string | null>(null);
  let previewPlaying = $state(false);
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  const keptFrom = $derived((clip.trimStartMs ?? 0) / 1000);
  const keptTo = $derived(clip.trimEndMs === null ? null : clip.trimEndMs / 1000);

  function startHover() {
    if (clip.kind === "screenshot" || clip.fileMissing || selecting || busy) return;
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(async () => {
      try {
        const url = await api.clipPreviewUrl(clip.id);
        if (hoverTimer !== undefined) previewUrl = url;
      } catch {
        // No preview; the picture stays.
      }
    }, HOVER_DELAY_MS);
  }

  let previewVideo = $state<HTMLVideoElement>();

  function stopHover() {
    clearTimeout(hoverTimer);
    hoverTimer = undefined;
    releaseVideo(previewVideo);
    previewUrl = null;
    previewPlaying = false;
  }

  $effect(() => () => clearTimeout(hoverTimer));

  function previewLoaded(e: Event) {
    (e.currentTarget as HTMLVideoElement).currentTime = keptFrom;
  }

  /** Loops the kept part instead of the whole recording. */
  function previewTime(e: Event) {
    const video = e.currentTarget as HTMLVideoElement;
    if (keptTo !== null && video.currentTime >= keptTo) video.currentTime = keptFrom;
  }

  function closeMenuOutside(e: MouseEvent) {
    if (menuOpen && !menuWrap?.contains(e.target as Node)) menuOpen = false;
  }

  /** Opens the clip, or selects it while selecting or with Ctrl/Shift held. */
  function open(e: MouseEvent, action: () => void) {
    if (onselect && (selecting || e.ctrlKey || e.metaKey || e.shiftKey)) {
      e.preventDefault();
      onselect(e);
    } else {
      // The card's screen is hidden, not removed, so a preview left running would play on behind.
      stopHover();
      action();
    }
  }

  /** Right-click: the same menu as ⋯ (a card without one keeps the usual right-click). */
  function oncontextmenu(e: MouseEvent) {
    if (clip.fileMissing) return;
    e.preventDefault();
    menuOpen = true;
  }

  function pick(action: () => void) {
    menuOpen = false;
    action();
  }
  const pickVisibility = (v: Visibility) => pick(() => onvisibility?.(v));
</script>

<svelte:window onclick={closeMenuOutside} oncontextmenu={closeMenuOutside} />

{#snippet linkOnly()}
  <button
    class="link-only"
    onclick={(e) => {
      e.stopPropagation();
      onvisibility?.("friends");
    }}
    disabled={busy || !onvisibility}
    title="Only people with the link can see it. Click to show it to your friends"
  >
    Link only
  </button>
{/snippet}

{#snippet selectPill()}
  <Icon name="check" size={15} />
  {selected ? "Unselect" : "Select"}
{/snippet}

<article class="card clip-card rise" class:missing={clip.fileMissing} class:menu-open={menuOpen} class:selected class:selecting class:previewing={previewPlaying} style:--i={index} {oncontextmenu}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="thumb" onmouseenter={startHover} onmouseleave={stopHover}>
    {#if onselect}
      <button
        class="select"
        role="checkbox"
        aria-checked={selected}
        aria-label="Select {clip.title}"
        title={selected ? "Selected" : "Select (or Ctrl+click the picture)"}
        onclick={onselect}
      >
        <Icon name="check" size={14} />
      </button>
    {/if}
    {#if thumb}
      <img src={thumb} alt="" loading="lazy" decoding="async" onerror={() => (thumbFailed = true)} />
    {:else}
      <div class="placeholder"><Icon name={clip.fileMissing ? "alert" : "film"} size={30} /></div>
    {/if}
    {#if previewUrl}
      <video
        bind:this={previewVideo}
        class="preview"
        class:shown={previewPlaying}
        src={previewUrl}
        muted
        autoplay
        loop
        playsinline
        disablepictureinpicture
        onloadedmetadata={previewLoaded}
        onplaying={() => (previewPlaying = true)}
        ontimeupdate={previewTime}
      ></video>
    {/if}
    {#if onview && !clip.fileMissing}
      <button class="open" onclick={(e) => open(e, onview)} aria-label="View {clip.title}">
        <span class="open-pill">{#if selecting}{@render selectPill()}{:else}<Icon name="image" size={15} /> View{/if}</span>
      </button>
    {:else if onplay}
      <button class="open" onclick={(e) => open(e, onplay)} aria-label="Play {clip.title}">
        <span class="open-pill">{#if selecting}{@render selectPill()}{:else}<Icon name="play" size={15} /> Play{/if}</span>
      </button>
    {:else if onedit && !clip.fileMissing}
      <!-- Not uploaded yet: the whole picture opens the editor, which is also where the clip plays. -->
      <button class="open" onclick={(e) => open(e, onedit)} disabled={busy && !selecting} aria-label="Edit {clip.title}">
        <span class="open-pill">{#if selecting}{@render selectPill()}{:else}<Icon name="scissors" size={15} /> Edit{/if}</span>
      </button>
    {/if}
    {#if clip.fileMissing}
      <span class="badge badge-missing"><Icon name="alert" size={13} /> File missing</span>
    {:else}
      {@const where = WHERE[uploaded ? "both" : "localOnly"]}
      <span class="badge badge-icon {uploaded ? 'badge-uploaded' : 'badge-local'}" role="img" aria-label={where.title} title={where.title}>
        <Icon name={where.icon} size={14} />
      </span>
    {/if}
    {#if uploaded && clip.visibility === "link" && !clip.fileMissing}
      {@render linkOnly()}
    {:else if uploaded && clip.visibility === "public" && !clip.fileMissing}
      <span class="link-only public" title="Public: anyone can find it on your profile"><Icon name="globe" size={12} /> Public</span>
    {/if}
    <ProcessingPill state={processing} />
    {#if clip.kind === "screenshot"}
      <span class="duration" title="Screenshot"><Icon name="image" size={13} /></span>
    {:else}
      <span class="duration" class:edited={length !== recorded} title={clip.isRecording ? "Recording" : undefined}>
        {#if clip.isRecording}<Icon name="record" size={12} />{/if}
        {#if length !== recorded}
          <Icon name="scissors" size={12} />
          {length}
          <s aria-label="recorded {recorded}">{recorded}</s>
        {:else}
          {length}
        {/if}
      </span>
    {/if}
    {#if onfavorite}
      <button
        class="star"
        class:on={clip.favorite}
        onclick={onfavorite}
        aria-pressed={clip.favorite}
        aria-label={clip.favorite ? "Remove from favorites" : "Add to favorites"}
        title={clip.favorite ? "Favorite: kept by the automatic cleanup" : "Add to favorites"}
      >
        <Icon name={clip.favorite ? "starFilled" : "star"} size={16} />
      </button>
    {/if}

    {#if job && (busy || job.status === "failed" || job.status === "needsLogin")}
      <div class="overlay">
        <span class="status" class:failed={job.status === "failed"}>{statusLabel(job)}</span>
        {#if job.status === "queued"}
          <div class="overlay-actions">
            <button class="btn btn-sm" onclick={() => forgetUpload(clip.id)}>Don't upload</button>
          </div>
        {:else if busy}
          <div class="progress" class:indeterminate={progress === null} style:--value={progress ?? undefined}>
            <div></div>
          </div>
          {#if transferLabel(job)}<span class="transfer">{transferLabel(job)}</span>{/if}
        {:else if job.status === "failed"}
          <div class="overlay-actions">
            <button class="btn btn-sm btn-primary" onclick={() => retryUpload(clip.id)}>
              <Icon name="retry" size={15} /> Try again
            </button>
            <button class="btn btn-sm" onclick={() => forgetUpload(clip.id)}>Dismiss</button>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <div class="info">
    <h3 title={clip.title}>{clip.title}</h3>
    <p class="meta">
      {#if clip.gameName}<span class="game-dot" style:--hue={hueOf(clip.gameName)}></span>{/if}
      <span class="game" class:unknown={!clip.gameName}>{clip.gameName ?? "Unknown game"}</span>
      <span aria-hidden="true">·</span>
      <span class="when">{relativeDate(clip.createdAt, now)}</span>
    </p>
  </div>

  <div class="actions">
    {#if clip.fileMissing}
      {#if uploaded}
        <button class="btn btn-sm grow" onclick={oncopy}><Icon name="link" size={15} /> Copy link</button>
        <button class="btn btn-sm btn-ghost btn-icon" onclick={onremove} aria-label="Remove from library" title="Remove from library">
          <Icon name="trash" size={16} />
        </button>
      {:else}
        <button class="btn btn-sm grow" onclick={onremove}><Icon name="trash" size={15} /> Remove from library</button>
      {/if}
    {:else}
      {#if uploaded}
        <button class="btn btn-sm grow" onclick={oncopy}><Icon name="link" size={15} /> Copy link</button>
      {:else if onedit}
        <!-- Clips are trimmed before they're shared, so editing leads; a straight upload is in the menu. -->
        <button class="btn btn-sm btn-soft grow" onclick={onedit} disabled={busy} title="Trim and volume">
          <Icon name="scissors" size={15} /> Edit
        </button>
      {:else}
        <button class="btn btn-sm btn-soft grow" onclick={onupload} disabled={busy}>
          <Icon name="upload" size={15} /> Upload
        </button>
      {/if}
      {#if onedit && uploaded}
        <button class="btn btn-sm btn-ghost btn-icon" onclick={onedit} disabled={busy} aria-label="Edit" title="Trim and volume">
          <Icon name="scissors" size={16} />
        </button>
      {/if}
      <div class="menu-wrap" bind:this={menuWrap}>
        <button
          class="btn btn-sm btn-ghost btn-icon"
          aria-label="More actions"
          aria-haspopup="menu"
          aria-expanded={menuOpen}
          onclick={() => (menuOpen = !menuOpen)}
        >
          <Icon name="more" />
        </button>
        {#if menuOpen}
          <div class="menu" role="menu" use:menu={{ close: () => (menuOpen = false) }}>
            <button role="menuitem" onclick={() => pick(onrename)}><Icon name="rename" size={16} /> Rename</button>
            <button role="menuitem" onclick={() => pick(onsetgame)}><Icon name="tag" size={16} /> Set game</button>
            <button role="menuitem" onclick={() => pick(onshowfolder)}>
              <Icon name="folder" size={16} /> Show in folder
            </button>
            {#if uploaded && onvisibility}
              <hr />
              {#each VISIBILITY as v (v.value)}
              <button role="menuitemradio" aria-checked={toVisibility(clip.visibility) === v.value} onclick={() => pickVisibility(v.value)} disabled={busy}>
                <Icon name={v.icon} size={16} />
                {v.label}{#if toVisibility(clip.visibility) === v.value} <Icon name="check" size={14} />{/if}
              </button>
            {/each}
              <hr />
            {/if}
            {#if uploaded && onalbum}
              <button role="menuitem" onclick={() => pick(onalbum)}><Icon name="album" size={16} /> Add to album</button>
            {/if}
            {#if uploaded}
              <button role="menuitem" onclick={() => pick(onupload)} disabled={busy}>
                <Icon name="upload" size={16} /> Upload again
              </button>
            {:else if onedit}
              <button role="menuitem" onclick={() => pick(onupload)} disabled={busy}>
                <Icon name="upload" size={16} /> Upload full clip
              </button>
            {/if}
            <hr />
            <button role="menuitem" class="danger" onclick={() => pick(onremove)} disabled={busy}>
              <Icon name="trash" size={16} /> Remove
            </button>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</article>
