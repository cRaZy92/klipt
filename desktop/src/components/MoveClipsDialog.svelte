<script lang="ts">
  import { api, errorMessage } from "../lib/api";
  import { formatSize } from "../lib/format";
  import { detail, jobs, percent } from "../lib/jobs.svelte";
  import Modal from "./Modal.svelte";

  // After the clips folder changed: offer to bring the clips in the old folder along, then cover
  // Klipt with the move's progress until it ends (uploads, exports and the cleanup wait for it).
  let {
    from,
    count,
    bytes,
    onclose,
  }: {
    /** The old clips folder. */
    from: string;
    count: number;
    bytes: number;
    onclose: () => void;
  } = $props();

  let jobId = $state<number | null>(null);
  let starting = $state(false);
  let refused = $state<string | null>(null);
  let cancelling = $state(false);

  const job = $derived(jobId === null ? null : (jobs.list.find((j) => j.id === jobId) ?? null));
  const running = $derived(starting || (jobId !== null && job?.state !== "done" && job?.state !== "failed" && job?.state !== "cancelled"));
  const clips = (n: number) => (n === 1 ? "1 clip" : `${n} clips`);

  async function move() {
    starting = true;
    refused = null;
    try {
      jobId = await api.moveClips(from);
    } catch (e) {
      refused = errorMessage(e);
    } finally {
      starting = false;
    }
  }

  async function cancel() {
    cancelling = true;
    await api.cancelMoveClips().catch(() => {});
  }

  // While it moves, the dialog can't be closed (Cancel stops it after the current file).
  function close() {
    if (!running) onclose();
  }
</script>

<Modal title={jobId === null ? "Move your clips too?" : "Moving your clips"} onclose={close} closeOnBackdrop={false}>
  {#if jobId === null}
    <p>
      <strong>{clips(count)}</strong> ({formatSize(bytes)}) are still in the old folder. Move them to the new one?
    </p>
    <p class="muted small" title={from}>From {from}</p>
    {#if refused}<p class="error">{refused}</p>{/if}
  {:else if job}
    <div class="progress" style:--value={job.progress}><div></div></div>
    <p class="status">
      <strong>{percent(job)}</strong>
      {#if detail(job)}<span class="muted"> · {detail(job)}</span>{/if}
    </p>
    {#if job.state === "running"}
      <p class="muted small">Uploads, exports and the cleanup wait until this is done.</p>
    {:else if job.message}
      <p class:error={job.state === "failed"}>{job.message}</p>
    {/if}
  {:else}
    <p class="muted"><span class="spinner" aria-hidden="true"></span> Starting…</p>
  {/if}

  {#snippet footer()}
    {#if jobId === null}
      <button class="btn btn-ghost" onclick={onclose} disabled={starting}>Leave them</button>
      <button class="btn btn-primary" onclick={move} disabled={starting}>
        {#if starting}<span class="spinner"></span>{/if}
        {refused ? "Try again" : "Move"}
      </button>
    {:else if running}
      <button class="btn" onclick={cancel} disabled={cancelling}>{cancelling ? "Stopping…" : "Cancel"}</button>
    {:else}
      <button class="btn btn-primary" onclick={onclose}>Close</button>
    {/if}
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 0.75rem;
  }

  .small {
    overflow: hidden;
    font-size: 0.85rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status {
    margin-top: 0.75rem;
    font-variant-numeric: tabular-nums;
  }

  .error {
    color: var(--danger-text);
  }
</style>
