<script lang="ts">
  import { api, errorMessage, type ClipView, type Visibility } from "../lib/api";
  import { uploadTitle } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import { fraction, isBusy, retryUpload, startUpload, statusLabel, uploads } from "../lib/uploads.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  let { clip, onclose }: { clip: ClipView; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let title = $state(uploadTitle(clip));
  let visibility = $state<Visibility>("feed");
  // svelte-ignore state_referenced_locally
  let started = $state(isBusy(uploads[clip.id]));

  const job = $derived(started ? uploads[clip.id] : undefined);
  const busy = $derived(isBusy(job));
  const progress = $derived(job ? fraction(job) : null);
  const trimmedTitle = $derived(title.trim());

  const STEPS = [
    { status: "exporting", label: "Prepare" },
    { status: "uploading", label: "Upload" },
    { status: "finalizing", label: "Finish" },
  ] as const;
  const stepIndex = $derived(
    job?.status === "done" ? 3 : Math.max(0, STEPS.findIndex((s) => s.status === job?.status)),
  );

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!trimmedTitle) return;
    started = true;
    void startUpload(clip.id, trimmedTitle, visibility);
  }

  async function copyAgain() {
    try {
      await api.copyShareLink(clip.id);
      toast("Link copied!", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }
</script>

<Modal title={job?.status === "done" ? "Clip uploaded" : "Upload clip"} {onclose}>
  {#if !job}
    <form id="upload-form" onsubmit={submit}>
      <label class="field">
        <span>Title</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={title} maxlength="100" autofocus />
      </label>
      <fieldset>
        <legend>Who can see it</legend>
        <label class="option" class:selected={visibility === "feed"}>
          <input type="radio" name="visibility" value="feed" bind:group={visibility} />
          <span>
            <strong>Feed</strong>
            <small>Your friends see it in the feed.</small>
          </span>
        </label>
        <label class="option" class:selected={visibility === "link"}>
          <input type="radio" name="visibility" value="link" bind:group={visibility} />
          <span>
            <strong>Link only</strong>
            <small>Only people you send the link to can watch it.</small>
          </span>
        </label>
      </fieldset>
    </form>
  {:else if job.status === "done"}
    <div class="done">
      <div class="done-icon"><Icon name="check" size={28} /></div>
      <p>{job.copied ? "The link is copied — paste it anywhere to share." : "Your clip is online."}</p>
      {#if job.shareUrl}<code class="link">{job.shareUrl}</code>{/if}
    </div>
  {:else}
    <div class="steps">
      {#each STEPS as step, i (step.status)}
        <div class="step" class:active={i === stepIndex && busy} class:complete={i < stepIndex}>
          <span class="bullet">{#if i < stepIndex}<Icon name="check" size={13} />{:else}{i + 1}{/if}</span>
          {step.label}
        </div>
      {/each}
    </div>
    <p class="status" class:failed={job.status === "failed"} role={job.status === "failed" ? "alert" : "status"}>
      {statusLabel(job)}
    </p>
    {#if busy}
      <div class="progress" class:indeterminate={progress === null} style:--value={progress ?? undefined}>
        <div></div>
      </div>
      <p class="muted small">You can close this window — the upload keeps going.</p>
    {/if}
  {/if}

  {#snippet footer()}
    {#if !job}
      <button class="btn" type="button" onclick={onclose}>Cancel</button>
      <button class="btn btn-primary" type="submit" form="upload-form" disabled={!trimmedTitle}>
        <Icon name="upload" size={16} /> Upload
      </button>
    {:else if job.status === "done"}
      {#if !job.copied}
        <button class="btn" onclick={copyAgain}><Icon name="link" size={16} /> Copy link</button>
      {/if}
      <button class="btn btn-primary" onclick={onclose}>Done</button>
    {:else if job.status === "failed"}
      <button class="btn" onclick={onclose}>Close</button>
      <button class="btn btn-primary" onclick={() => retryUpload(clip.id)}><Icon name="retry" size={16} /> Try again</button>
    {:else}
      <button class="btn" onclick={onclose}>Hide</button>
    {/if}
  {/snippet}
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    margin-bottom: 0.4rem;
    color: var(--text-muted);
    font-size: 0.87rem;
    font-weight: 600;
  }

  .option {
    display: flex;
    align-items: flex-start;
    gap: 0.7rem;
    padding: 0.75rem 0.85rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition:
      border-color 0.12s,
      background 0.12s;
  }

  .option.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .option input {
    margin: 0.2rem 0 0;
    accent-color: var(--accent);
  }

  .option span {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  small {
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  .steps {
    display: flex;
    gap: 1.25rem;
  }

  .step {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    color: var(--text-faint);
    font-size: 0.88rem;
    font-weight: 600;
  }

  .step.active {
    color: var(--text);
  }

  .step.complete {
    color: var(--success);
  }

  .bullet {
    display: grid;
    place-items: center;
    width: 1.4rem;
    height: 1.4rem;
    border: 1px solid currentColor;
    border-radius: 50%;
    font-size: 0.75rem;
  }

  .step.active .bullet {
    border-color: var(--accent);
    background: var(--accent);
    color: #fff;
  }

  .status {
    font-weight: 600;
  }

  .status.failed {
    color: #ffb3ba;
    font-weight: 500;
  }

  .small {
    font-size: 0.85rem;
  }

  .done {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    text-align: center;
  }

  .done-icon {
    display: grid;
    place-items: center;
    width: 3.2rem;
    height: 3.2rem;
    border-radius: 50%;
    background: var(--success-soft);
    color: var(--success);
  }

  .link {
    max-width: 100%;
    overflow: hidden;
    padding: 0.5rem 0.7rem;
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text-muted);
    font-size: 0.85rem;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: all;
  }
</style>
