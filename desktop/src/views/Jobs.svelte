<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { toast } from "../lib/toasts.svelte";
  import { errorMessage } from "../lib/api";
  import { clearFinished, detail, jobs, KIND_ICONS, kindLabel, ordered, percent, timing } from "../lib/jobs.svelte";

  // Everything Klipt works on in the background, with how far it is. Finished jobs stay until
  // Clear finished or the next start of Klipt.

  let { active = true }: { active?: boolean } = $props();

  const list = $derived(ordered(jobs.list));
  const finished = $derived(list.filter((j) => j.state !== "running").length);

  // "1:35 so far" ticks while the screen is shown and something runs.
  let now = $state(Date.now());
  $effect(() => {
    if (!active || finished === list.length) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  async function clear() {
    try {
      await clearFinished();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }
</script>

<section class="page">
  <div class="page-top">
    <header class="page-head">
      <div>
        <h1>Jobs</h1>
        <p class="subtitle">What Klipt is working on: uploads, imports, exports and more. Finished ones stay until you clear them.</p>
      </div>
      <div class="page-tools">
        <button class="btn" onclick={clear} disabled={finished === 0}>Clear finished</button>
      </div>
    </header>
  </div>

  {#if list.length === 0}
    <div class="empty">
      <div class="empty-icon"><Icon name="pulse" size={28} /></div>
      <h2>Nothing going on</h2>
      <p>Uploads, imports, exports and downloads show up here while they run.</p>
    </div>
  {:else}
    <ul class="jobs">
      {#each list as job (job.id)}
        {@const more = detail(job)}
        <li class="job {job.state}">
          <span class="kind-icon" aria-hidden="true"><Icon name={KIND_ICONS[job.kind]} size={18} /></span>
          <div class="body">
            <div class="top">
              <span class="name">
                <span class="kind">{kindLabel(job.kind)}</span>
                <strong title={job.title}>{job.title}</strong>
              </span>
              <span class="pct">{percent(job)}</span>
            </div>
            <div class="progress" style:--value={job.progress}><div></div></div>
            <div class="bottom">
              <span class="step">
                {#if job.state === "running"}<span class="spinner" aria-hidden="true"></span>{/if}
                {#if job.state === "failed"}<Icon name="alert" size={14} />{/if}
                {#if job.state === "done"}<Icon name="check" size={14} />{/if}
                {job.step}{#if more}<span class="muted"> · {more}</span>{/if}
              </span>
              <span class="when muted">{timing(job, now)}</span>
            </div>
            {#if job.message}<p class="message">{job.message}</p>{/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .jobs {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .job {
    display: flex;
    gap: 0.85rem;
    padding: 0.9rem 1rem;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
  }

  .kind-icon {
    display: grid;
    flex: none;
    width: 2.25rem;
    height: 2.25rem;
    place-items: center;
    border-radius: 10px;
    background: var(--surface-3);
    color: var(--accent-text);
  }

  .body {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 0.45rem;
    min-width: 0;
  }

  .top,
  .bottom {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
  }

  .name {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    min-width: 0;
  }

  .name strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .kind {
    flex: none;
    color: var(--text-muted);
    font-size: 0.8rem;
    font-weight: 600;
  }

  .pct {
    flex: none;
    font-variant-numeric: tabular-nums;
    font-weight: 700;
  }

  .bottom {
    font-size: 0.85rem;
  }

  .step {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
  }

  .step .spinner {
    width: 0.8rem;
    height: 0.8rem;
  }

  .when {
    flex: none;
    font-variant-numeric: tabular-nums;
  }

  .message {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  .job.done .progress > div {
    background: var(--success);
    box-shadow: none;
  }

  .job.done .step {
    color: var(--success);
  }

  .job.failed {
    border-color: color-mix(in srgb, var(--danger) 40%, var(--border));
  }

  .job.failed .progress > div,
  .job.cancelled .progress > div {
    background: var(--text-muted);
    box-shadow: none;
  }

  .job.failed .step,
  .job.failed .message {
    color: var(--danger-text);
  }
</style>
