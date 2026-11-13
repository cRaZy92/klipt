<script lang="ts">
  import { removalChoices, type RemoveWhere } from "../lib/removal";
  import Modal from "./Modal.svelte";

  // "Delete clip?" for one clip or several (My clips' selection, the editor): from the library
  // only, also the files on this PC, or also the uploads. The caller does the deleting.
  let {
    local,
    online = [],
    busy = false,
    onconfirm,
    onclose,
  }: {
    /** Clips in the library on this PC. */
    local: { title: string; fileMissing: boolean; uploaded: boolean }[];
    /** Uploads that aren't on this PC (only "Delete everywhere" touches them). */
    online?: { title: string }[];
    /** Deleting: the buttons wait. */
    busy?: boolean;
    onconfirm: (where: RemoveWhere) => void;
    onclose: () => void;
  } = $props();

  const total = $derived(local.length + online.length);
  const one = $derived(total === 1);
  const choices = $derived(removalChoices({ local, onlineOnly: online.length }));
  // Picked once, when the dialog opens.
  // svelte-ignore state_referenced_locally
  let where = $state<RemoveWhere>(choices.initial);
</script>

<Modal title={one ? "Delete clip?" : `Delete ${total} clips?`} {onclose}>
  {#if one}
    <p><strong>{local[0]?.title ?? online[0]?.title}</strong></p>
  {:else if online.length > 0 && local.length > 0}
    <p class="muted">{local.length} on this PC, {online.length} only online.</p>
  {/if}
  <div class="pick-list" role="radiogroup" aria-label="Where to delete from">
    {#if choices.library}
      <label class="pick">
        <input type="radio" bind:group={where} value="library" />
        <span>
          <strong>Remove from library only</strong>
          <small>
            {#if one}
              {local[0].fileMissing ? "Its file is already gone" : "The file stays in its folder"}{local[0].uploaded
                ? " and the link keeps working"
                : ""}.
            {:else}
              Files stay in their folders and links keep working.
            {/if}
          </small>
        </span>
      </label>
    {/if}
    {#if choices.local}
      <label class="pick">
        <input type="radio" bind:group={where} value="local" />
        <span>
          <strong>Delete from this PC</strong>
          <small>
            {#if one}
              {local[0].uploaded
                ? "The upload and its link keep working."
                : "The video file is deleted. It isn't uploaded, so it's gone for good."}
            {:else if choices.notUploaded > 0}
              Uploads keep working, but {choices.notUploaded === 1 ? "1 clip isn't" : `${choices.notUploaded} clips aren't`} uploaded
              and will be gone for good.
            {:else}
              The files are deleted. Uploads and their links keep working.
            {/if}
          </small>
        </span>
      </label>
    {/if}
    {#if choices.everywhere}
      <label class="pick danger">
        <input type="radio" bind:group={where} value="everywhere" />
        <span>
          <strong>{choices.library ? "Delete everywhere" : "Delete from the server"}</strong>
          <small>
            {choices.library ? "This PC and the server." : ""}
            {one ? "The link stops working" : "Links stop working"} for everyone.
          </small>
        </span>
      </label>
    {/if}
  </div>
  {#if online.length > 0 && where !== "everywhere"}
    <p class="muted hint">
      {online.length === 1 ? "1 clip is" : `${online.length} clips are`} only online, so only “Delete everywhere” deletes
      {online.length === 1 ? "it" : "them"}.
    </p>
  {/if}
  {#snippet footer()}
    <button class="btn" onclick={onclose} disabled={busy}>Cancel</button>
    <button class="btn btn-danger" onclick={() => onconfirm(where)} disabled={busy}>
      {#if busy}<span class="spinner"></span>{/if}
      {where === "library" ? "Remove" : "Delete"}
    </button>
  {/snippet}
</Modal>

<style>
  .hint {
    margin-top: 0.75rem;
    font-size: 0.86rem;
  }
</style>
