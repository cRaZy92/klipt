<script lang="ts">
  import { fileUrl, type ClipView } from "../lib/api";
  import { formatDuration, relativeDate } from "../lib/format";
  import { forgetUpload, fraction, isBusy, retryUpload, statusLabel, uploads } from "../lib/uploads.svelte";
  import Icon from "./Icon.svelte";

  let {
    clip,
    now,
    onupload,
    onedit,
    onrename,
    onsetgame,
    onremove,
    onshowfolder,
    oncopy,
  }: {
    clip: ClipView;
    /** The time relative dates are counted from (ticks once a minute). */
    now: number;
    onupload: () => void;
    /** The editor (Task 11). The Edit action is hidden until it's wired. */
    onedit?: () => void;
    onrename: () => void;
    onsetgame: () => void;
    onremove: () => void;
    onshowfolder: () => void;
    oncopy: () => void;
  } = $props();

  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();
  let thumbFailed = $state(false);

  const job = $derived(uploads[clip.id]);
  const busy = $derived(isBusy(job));
  const uploaded = $derived(!!clip.shareUrl);
  const thumb = $derived(!clip.fileMissing && clip.thumbPath && !thumbFailed ? fileUrl(clip.thumbPath) : null);
  const progress = $derived(job ? fraction(job) : null);

  function closeMenuOutside(e: MouseEvent) {
    if (menuOpen && !menuWrap?.contains(e.target as Node)) menuOpen = false;
  }

  function pick(action: () => void) {
    menuOpen = false;
    action();
  }
</script>

<svelte:window onclick={closeMenuOutside} />

<article class="card" class:missing={clip.fileMissing}>
  <div class="thumb">
    {#if thumb}
      <img src={thumb} alt="" loading="lazy" onerror={() => (thumbFailed = true)} />
    {:else}
      <div class="placeholder"><Icon name={clip.fileMissing ? "alert" : "film"} size={30} /></div>
    {/if}
    {#if clip.fileMissing}
      <span class="badge badge-missing">File missing</span>
    {:else if uploaded}
      <span class="badge badge-uploaded"><Icon name="check" size={13} /> Uploaded</span>
    {/if}
    <span class="duration">{formatDuration(clip.durationMs)}</span>

    {#if job && (busy || job.status === "failed" || job.status === "needsLogin")}
      <div class="overlay">
        <span class="status" class:failed={job.status === "failed"}>{statusLabel(job)}</span>
        {#if busy}
          <div class="progress" class:indeterminate={progress === null} style:--value={progress ?? undefined}>
            <div></div>
          </div>
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
      <span class:unknown={!clip.gameName}>{clip.gameName ?? "Unknown game"}</span>
      <span aria-hidden="true">·</span>
      <span>{relativeDate(clip.createdAt, now)}</span>
    </p>
  </div>

  <div class="actions">
    {#if clip.fileMissing}
      {#if uploaded}
        <button class="btn btn-sm grow" onclick={oncopy}><Icon name="link" size={15} /> Copy link</button>
      {/if}
      <button class="btn btn-sm grow" onclick={onremove}><Icon name="trash" size={15} /> Remove from library</button>
    {:else}
      {#if uploaded}
        <button class="btn btn-sm grow" onclick={oncopy}><Icon name="link" size={15} /> Copy link</button>
      {:else}
        <button class="btn btn-sm btn-primary grow" onclick={onupload} disabled={busy}>
          <Icon name="upload" size={15} /> Upload
        </button>
      {/if}
      {#if onedit}
        <button class="btn btn-sm" onclick={onedit} disabled={busy}><Icon name="scissors" size={15} /> Edit</button>
      {/if}
      <div class="menu-wrap" bind:this={menuWrap}>
        <button
          class="btn btn-sm btn-icon"
          aria-label="More actions"
          aria-haspopup="menu"
          aria-expanded={menuOpen}
          onclick={() => (menuOpen = !menuOpen)}
        >
          <Icon name="more" />
        </button>
        {#if menuOpen}
          <div class="menu" role="menu">
            <button role="menuitem" onclick={() => pick(onrename)}><Icon name="rename" size={16} /> Rename</button>
            <button role="menuitem" onclick={() => pick(onsetgame)}><Icon name="tag" size={16} /> Set game</button>
            <button role="menuitem" onclick={() => pick(onshowfolder)}>
              <Icon name="folder" size={16} /> Show in folder
            </button>
            {#if uploaded}
              <button role="menuitem" onclick={() => pick(onupload)} disabled={busy}>
                <Icon name="upload" size={16} /> Upload again
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

<style>
  .card {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    transition:
      border-color 0.15s,
      transform 0.15s;
  }

  .card:hover {
    border-color: #3a4150;
  }

  .thumb {
    position: relative;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border-radius: var(--radius) var(--radius) 0 0;
    background: var(--surface-2);
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .placeholder {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--text-faint);
  }

  .missing .placeholder {
    color: var(--warning);
  }

  .badge {
    position: absolute;
    top: 0.5rem;
    left: 0.5rem;
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.2rem 0.5rem;
    border-radius: 6px;
    font-size: 0.75rem;
    font-weight: 700;
    backdrop-filter: blur(4px);
  }

  .badge-uploaded {
    background: rgba(16, 40, 30, 0.85);
    color: var(--success);
  }

  .badge-missing {
    background: rgba(48, 34, 10, 0.88);
    color: var(--warning);
  }

  .duration {
    position: absolute;
    right: 0.5rem;
    bottom: 0.5rem;
    padding: 0.1rem 0.4rem;
    border-radius: 5px;
    background: rgba(0, 0, 0, 0.72);
    font-size: 0.78rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: stretch;
    gap: 0.6rem;
    padding: 1rem 1.25rem;
    background: rgba(10, 11, 15, 0.8);
    text-align: center;
  }

  .overlay-actions {
    display: flex;
    justify-content: center;
    gap: 0.4rem;
  }

  .status {
    font-size: 0.88rem;
    font-weight: 600;
  }

  .status.failed {
    color: #ffb3ba;
    font-weight: 500;
  }

  .info {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    padding: 0.75rem 0.85rem 0.5rem;
    min-width: 0;
  }

  h3 {
    overflow: hidden;
    font-size: 0.97rem;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    gap: 0.35rem;
    overflow: hidden;
    color: var(--text-muted);
    font-size: 0.85rem;
    white-space: nowrap;
  }

  .meta span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .unknown {
    font-style: italic;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    padding: 0.35rem 0.85rem 0.85rem;
    margin-top: auto;
  }

  .grow {
    flex: 1;
  }

  .menu-wrap {
    position: relative;
  }

  .menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + 0.35rem);
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 11rem;
    padding: 0.3rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    box-shadow: var(--shadow);
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.5rem 0.6rem;
    border: none;
    border-radius: 6px;
    background: none;
    font-size: 0.9rem;
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover:not(:disabled) {
    background: var(--surface-3);
  }

  .menu button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .menu .danger {
    color: var(--danger);
  }

  hr {
    width: 100%;
    margin: 0.25rem 0;
    border: none;
    border-top: 1px solid var(--border);
  }
</style>
