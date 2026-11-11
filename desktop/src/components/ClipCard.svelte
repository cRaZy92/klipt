<script lang="ts">
  import { fileUrl, type ClipView } from "../lib/api";
  import { formatDuration, hueOf, relativeDate } from "../lib/format";
  import { forgetUpload, fraction, isBusy, retryUpload, statusLabel, uploads } from "../lib/uploads.svelte";
  import { menu } from "../lib/a11y";
  import Icon from "./Icon.svelte";

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

<article class="card rise" class:missing={clip.fileMissing} class:menu-open={menuOpen} style:--i={index}>
  <div class="thumb">
    {#if thumb}
      <img src={thumb} alt="" loading="lazy" onerror={() => (thumbFailed = true)} />
    {:else}
      <div class="placeholder"><Icon name={clip.fileMissing ? "alert" : "film"} size={30} /></div>
    {/if}
    {#if onedit && !clip.fileMissing}
      <!-- The whole picture opens the editor, which is also where the clip plays. -->
      <button class="open" onclick={onedit} disabled={busy} aria-label="Edit {clip.title}">
        <span class="open-pill"><Icon name="scissors" size={15} /> Edit</span>
      </button>
    {/if}
    {#if clip.fileMissing}
      <span class="badge badge-missing"><Icon name="alert" size={13} /> File missing</span>
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
      {:else}
        <button class="btn btn-sm btn-soft grow" onclick={onupload} disabled={busy}>
          <Icon name="upload" size={15} /> Upload
        </button>
      {/if}
      {#if onedit}
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
    position: relative;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    transition:
      border-color 0.2s,
      box-shadow 0.2s,
      transform 0.2s var(--ease-out);
  }

  .card:hover {
    border-color: var(--border-strong);
    box-shadow: 0 16px 36px -18px rgba(0, 0, 0, 0.8);
    transform: translateY(-2px);
  }

  /* Its menu must draw over the neighbouring cards. */
  .card.menu-open {
    z-index: 5;
  }

  .thumb {
    position: relative;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border-radius: calc(var(--radius) - 1px) calc(var(--radius) - 1px) 0 0;
    background: var(--surface-2);
  }

  /* Darkens the bottom so the duration stays readable on bright frames. */
  .thumb::after {
    content: "";
    position: absolute;
    inset: auto 0 0;
    height: 45%;
    background: linear-gradient(transparent, rgba(5, 4, 10, 0.55));
    pointer-events: none;
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    transition: transform 0.45s var(--ease-out);
  }

  .card:hover img {
    transform: scale(1.05);
  }

  .placeholder {
    display: grid;
    place-items: center;
    height: 100%;
    background:
      radial-gradient(circle at 50% 40%, rgba(124, 92, 255, 0.12), transparent 60%),
      var(--surface-2);
    color: var(--text-faint);
  }

  .missing .placeholder {
    background: var(--surface-2);
    color: var(--warning);
  }

  .open {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: grid;
    place-items: center;
    padding: 0;
    border: none;
    background: transparent;
    cursor: pointer;
    transition: background 0.2s;
  }

  .open:hover:not(:disabled),
  .open:focus-visible {
    background: rgba(8, 7, 14, 0.35);
  }

  .open:disabled {
    cursor: default;
  }

  .open:focus-visible {
    outline-offset: -3px;
    border-radius: inherit;
  }

  .open-pill {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.45rem 0.9rem;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 999px;
    background: rgba(15, 13, 24, 0.72);
    backdrop-filter: blur(8px);
    font-size: 0.88rem;
    font-weight: 650;
    opacity: 0;
    transform: translateY(4px) scale(0.96);
    transition:
      opacity 0.2s,
      transform 0.2s var(--ease-out);
  }

  .open:hover:not(:disabled) .open-pill,
  .open:focus-visible .open-pill {
    opacity: 1;
    transform: none;
  }

  .badge {
    position: absolute;
    top: 0.55rem;
    left: 0.55rem;
    z-index: 2;
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.22rem 0.55rem;
    border-radius: 999px;
    font-size: 0.74rem;
    font-weight: 700;
    backdrop-filter: blur(8px);
    pointer-events: none;
  }

  .badge-uploaded {
    border: 1px solid rgba(60, 207, 142, 0.3);
    background: rgba(8, 28, 20, 0.78);
    color: var(--success);
  }

  .badge-missing {
    border: 1px solid rgba(255, 181, 71, 0.3);
    background: rgba(40, 28, 8, 0.82);
    color: var(--warning);
  }

  .duration {
    position: absolute;
    right: 0.55rem;
    bottom: 0.5rem;
    z-index: 2;
    padding: 0.1rem 0.45rem;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.6);
    font-size: 0.78rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }

  .overlay {
    position: absolute;
    inset: 0;
    z-index: 3;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: stretch;
    gap: 0.65rem;
    padding: 1rem 1.25rem;
    background: rgba(9, 8, 15, 0.78);
    backdrop-filter: blur(3px);
    text-align: center;
  }

  .overlay-actions {
    display: flex;
    justify-content: center;
    gap: 0.4rem;
  }

  .status {
    font-size: 0.88rem;
    font-weight: 650;
  }

  .status.failed {
    color: var(--danger-text);
    font-weight: 500;
  }

  .info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.8rem 0.9rem 0.55rem;
    min-width: 0;
  }

  h3 {
    overflow: hidden;
    font-size: 0.98rem;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    overflow: hidden;
    color: var(--text-muted);
    font-size: 0.84rem;
    white-space: nowrap;
  }

  .game {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .when {
    flex: none;
  }

  .unknown {
    font-style: italic;
  }

  .actions {
    display: flex;
    gap: 0.3rem;
    margin-top: auto;
    padding: 0.3rem 0.6rem 0.75rem 0.9rem;
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
    bottom: calc(100% + 0.4rem);
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 11.5rem;
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
      transform: translateY(4px);
    }
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.5rem 0.6rem;
    border: none;
    border-radius: 8px;
    background: none;
    color: var(--text-muted);
    font-size: 0.9rem;
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text);
  }

  .menu button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .menu .danger {
    color: var(--danger);
  }

  .menu .danger:hover:not(:disabled) {
    background: var(--danger-soft);
    color: var(--danger);
  }

  hr {
    width: 100%;
    margin: 0.25rem 0;
    border: none;
    border-top: 1px solid var(--border);
  }
</style>
