<script lang="ts">
  import type { MyUpload } from "../lib/api";
  import { menu } from "../lib/a11y";
  import { formatDuration, hueOf, relativeDate, viewsLabel } from "../lib/format";
  import Icon from "./Icon.svelte";
  import ProcessingPill from "./ProcessingPill.svelte";
  import "./clip-card.css";

  // One of your uploads that isn't on this PC: it plays from the server and can be downloaded.
  let {
    upload,
    now,
    index = 0,
    downloading = false,
    onplay,
    oncopy,
    onopenweb,
    onrename,
    onvisibility,
    ondownload,
    ondelete,
    onfavorite,
    selected = false,
    selecting = false,
    onselect,
  }: {
    upload: MyUpload;
    now: number;
    index?: number;
    /** A download of this clip is running. */
    downloading?: boolean;
    onplay: () => void;
    oncopy: () => void;
    onopenweb: () => void;
    onrename: () => void;
    onvisibility: () => void;
    ondownload: () => void;
    ondelete: () => void;
    /** Stars it on the server; missing for servers without favorites. */
    onfavorite?: () => void;
    /** Picked for a bulk action. */
    selected?: boolean;
    /** Something is selected: clicking the picture selects instead of playing. */
    selecting?: boolean;
    onselect?: (e: MouseEvent) => void;
  } = $props();

  function open(e: MouseEvent) {
    if (onselect && (selecting || e.ctrlKey || e.metaKey || e.shiftKey)) {
      e.preventDefault();
      onselect(e);
    } else {
      onplay();
    }
  }

  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();
  let thumbFailed = $state(false);

  function closeMenuOutside(e: MouseEvent) {
    if (menuOpen && !menuWrap?.contains(e.target as Node)) menuOpen = false;
  }

  function pick(action: () => void) {
    menuOpen = false;
    action();
  }
</script>

<svelte:window onclick={closeMenuOutside} />

<article class="card clip-card rise" class:menu-open={menuOpen} class:selected class:selecting style:--i={index}>
  <div class="thumb">
    {#if onselect}
      <button
        class="select"
        role="checkbox"
        aria-checked={selected}
        aria-label="Select {upload.title}"
        title={selected ? "Selected" : "Select (or Ctrl+click the picture)"}
        onclick={onselect}
      >
        <Icon name="check" size={14} />
      </button>
    {/if}
    {#if !thumbFailed}
      <img src={upload.thumbnailUrl} alt="" loading="lazy" onerror={() => (thumbFailed = true)} />
    {:else}
      <div class="placeholder"><Icon name="film" size={30} /></div>
    {/if}
    <button class="open" onclick={open} aria-label="Play {upload.title}">
      <span class="open-pill">
        {#if selecting}<Icon name="check" size={15} /> {selected ? "Unselect" : "Select"}{:else}<Icon name="play" size={15} /> Play{/if}
      </span>
    </button>
    <span class="badge badge-online" title="Uploaded; not on this computer">
      <Icon name="cloud" size={13} /> Online only
    </span>
    <ProcessingPill state={upload.processing} />
    {#if onfavorite}
      <button
        class="star"
        class:on={upload.favorite}
        onclick={onfavorite}
        aria-pressed={!!upload.favorite}
        aria-label={upload.favorite ? "Remove from favorites" : "Add to favorites"}
        title={upload.favorite ? "In your favorites" : "Add to favorites"}
      >
        <Icon name={upload.favorite ? "starFilled" : "star"} size={16} />
      </button>
    {/if}
    <span class="duration">{upload.kind === "screenshot" ? "Screenshot" : formatDuration(upload.durationMs)}</span>
    {#if downloading}
      <div class="overlay">
        <span class="status">Downloading…</span>
        <div class="progress indeterminate"><div></div></div>
      </div>
    {/if}
  </div>

  <div class="info">
    <h3 title={upload.title}>{upload.title}</h3>
    <p class="meta">
      {#if upload.gameName}<span class="game-dot" style:--hue={hueOf(upload.gameName)}></span>{/if}
      <span class="game" class:unknown={!upload.gameName}>{upload.gameName ?? "Unknown game"}</span>
      <span aria-hidden="true">·</span>
      <span class="when">{relativeDate(new Date(upload.createdAt), now)}</span>
      {#if upload.views > 0}
        <span aria-hidden="true">·</span>
        <span class="when">{viewsLabel(upload.views)}</span>
      {/if}
      {#if upload.visibility === "link"}
        <span aria-hidden="true">·</span>
        <span class="when" title="Only people with the link can see it">Link only</span>
      {/if}
    </p>
  </div>

  <div class="actions">
    <button class="btn btn-sm grow" onclick={oncopy}><Icon name="link" size={15} /> Copy link</button>
    <button
      class="btn btn-sm btn-ghost btn-icon"
      onclick={ondownload}
      disabled={downloading}
      aria-label="Download to this PC"
      title="Download to this PC (to trim it or keep a copy)"
    >
      <Icon name="download" size={16} />
    </button>
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
          <button role="menuitem" onclick={() => pick(onopenweb)}><Icon name="external" size={16} /> Open on website</button>
          <button role="menuitem" onclick={() => pick(onrename)}><Icon name="rename" size={16} /> Rename</button>
          <button role="menuitem" onclick={() => pick(onvisibility)}>
            <Icon name="eye" size={16} />
            {upload.visibility === "feed" ? "Hide from feed" : "Show in feed"}
          </button>
          <button role="menuitem" onclick={() => pick(ondownload)} disabled={downloading}>
            <Icon name="download" size={16} /> Download to this PC
          </button>
          <hr />
          <button role="menuitem" class="danger" onclick={() => pick(ondelete)} disabled={downloading}>
            <Icon name="trash" size={16} /> Delete from server
          </button>
        </div>
      {/if}
    </div>
  </div>
</article>
