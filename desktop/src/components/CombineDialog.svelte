<script lang="ts">
  import { api, errorMessage, fileUrl, on, type ClipView } from "../lib/api";
  import { combinedLength, combinedTitle, moveItem } from "../lib/combine";
  import { formatDuration, uploadTitle } from "../lib/format";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  let {
    clips,
    onclose,
    ondone,
  }: {
    /** The clips to join (at least two), in the order suggested. */
    clips: ClipView[];
    onclose: () => void;
    /** The new clip is made; `upload`: the user asked to upload it next (and didn't close the dialog). */
    ondone: (clip: ClipView, upload: boolean) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let order = $state<ClipView[]>([...clips]);
  // svelte-ignore state_referenced_locally
  let title = $state(combinedTitle(clips));
  let working = $state(false);
  let progress = $state(0);
  let error = $state<string | null>(null);
  /** Closed while combining: the clip still arrives, but no upload dialog pops up. */
  let closed = false;

  const trimmedTitle = $derived(title.trim());
  const length = $derived(formatDuration(combinedLength(order)));

  $effect(() => on("combine-progress", (p) => (progress = p)));

  function close() {
    closed = true;
    onclose();
  }

  async function combine(upload: boolean) {
    if (!trimmedTitle || order.length < 2 || working) return;
    working = true;
    progress = 0;
    error = null;
    try {
      const clip = await api.combineClips(
        order.map((c) => c.id),
        trimmedTitle,
      );
      ondone(clip, upload && !closed);
    } catch (e) {
      error = errorMessage(e);
      working = false;
    }
  }
</script>

<Modal title="Combine {order.length} clips" onclose={close} closeOnBackdrop={false} width="32rem">
  {#if !working}
    <form
      id="combine-form"
      onsubmit={(e) => {
        e.preventDefault();
        void combine(true);
      }}
    >
      <label class="field">
        <span>Title</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={title} maxlength="100" autofocus />
      </label>
      <div class="order">
        <p class="label">Order <span class="muted">· {length} in all</span></p>
        <ol>
          {#each order as clip, i (clip.id)}
            <li>
              <span class="num">{i + 1}</span>
              {#if clip.thumbPath}
                <img src={fileUrl(clip.thumbPath)} alt="" loading="lazy" />
              {:else}
                <span class="thumb-empty"><Icon name="film" size={16} /></span>
              {/if}
              <span class="name">
                <strong>{uploadTitle(clip)}</strong>
                <small>{formatDuration(combinedLength([clip]))}{clip.gameName ? ` · ${clip.gameName}` : ""}</small>
              </span>
              <span class="actions">
                <button
                  type="button"
                  class="btn btn-ghost btn-icon btn-sm"
                  aria-label="Move {uploadTitle(clip)} up"
                  disabled={i === 0}
                  onclick={() => (order = moveItem(order, i, -1))}><Icon name="arrowUp" size={15} /></button
                >
                <button
                  type="button"
                  class="btn btn-ghost btn-icon btn-sm"
                  aria-label="Move {uploadTitle(clip)} down"
                  disabled={i === order.length - 1}
                  onclick={() => (order = moveItem(order, i, 1))}><Icon name="arrowDown" size={15} /></button
                >
                <button
                  type="button"
                  class="btn btn-ghost btn-icon btn-sm"
                  aria-label="Leave {uploadTitle(clip)} out"
                  title="Leave out"
                  disabled={order.length <= 2}
                  onclick={() => (order = order.filter((c) => c.id !== clip.id))}><Icon name="close" size={15} /></button
                >
              </span>
            </li>
          {/each}
        </ol>
      </div>
      <p class="muted small">
        Each clip goes in with its trim, cuts, zooms, subtitles and volumes. The result is a new clip in My clips; these
        clips stay as they are.
      </p>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
    </form>
  {:else}
    <p class="status" role="status">Joining {order.length} clips into “{trimmedTitle}”…</p>
    <div class="progress" style:--value={progress}><div></div></div>
    <p class="muted small">
      The picture is encoded again, so this takes a while for long clips. You can close this window: the new clip shows
      up in My clips when it's done.
    </p>
  {/if}

  {#snippet footer()}
    {#if !working}
      <button class="btn" type="button" onclick={close}>Cancel</button>
      <button class="btn" type="button" disabled={!trimmedTitle} onclick={() => combine(false)}>Just combine</button>
      <button class="btn btn-primary" type="submit" form="combine-form" disabled={!trimmedTitle}>
        <Icon name="upload" size={16} /> Combine and upload
      </button>
    {:else}
      <button class="btn" onclick={close}>Hide</button>
    {/if}
  {/snippet}
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .label {
    margin: 0 0 0.4rem;
    color: var(--text-muted);
    font-size: 0.87rem;
    font-weight: 600;
  }

  .label .muted {
    font-weight: 500;
  }

  ol {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    max-height: 16rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.35rem 0.4rem 0.35rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
  }

  .num {
    width: 1.2rem;
    color: var(--text-faint);
    font-size: 0.8rem;
    font-weight: 700;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  img,
  .thumb-empty {
    flex: none;
    width: 4rem;
    height: 2.25rem;
    border-radius: 6px;
    background: var(--surface-3);
    object-fit: cover;
  }

  .thumb-empty {
    display: grid;
    place-items: center;
    color: var(--text-faint);
  }

  .name {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .name strong,
  .name small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name strong {
    font-size: 0.9rem;
  }

  .name small {
    color: var(--text-muted);
    font-size: 0.78rem;
  }

  .actions {
    display: flex;
    flex: none;
  }

  .small {
    margin: 0;
    font-size: 0.82rem;
  }

  .status {
    margin: 0 0 0.75rem;
    font-weight: 600;
  }

  .progress {
    margin-bottom: 0.75rem;
  }

  .error {
    margin: 0;
    color: var(--danger-text);
    font-size: 0.87rem;
  }
</style>
