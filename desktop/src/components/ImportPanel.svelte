<script lang="ts">
  import { dismissImportSummary, importing, stopImport } from "../lib/imports.svelte";
  import { elapsedText, etaText } from "../lib/importSummary";
  import Icon from "./Icon.svelte";

  // My clips' import status: everything we know while it runs (overall %, time left, the file
  // being converted with its own % and speed, counts), and the skipped files once it's done.

  const p = $derived(importing.progress);
  let stopping = $state(false);

  $effect(() => {
    if (importing.phase !== "running") stopping = false;
  });

  const ACTION: Record<string, string> = { importing: "Importing", remuxing: "Copying", converting: "Converting" };

  function stop() {
    stopping = true;
    stopImport();
  }
</script>

{#if importing.phase === "running"}
  <section class="import-panel" role="status" aria-label="Import progress">
    <div class="top">
      <div class="title">
        <strong>Importing clips</strong>
        {#if p}
          <span class="pct">{Math.round(p.fraction * 100)}%</span>
        {/if}
      </div>
      <div class="times">
        {#if p}
          <span title="Time left, from the speed so far">{p.done >= p.total ? "Finishing…" : `${etaText(p.etaSecs)} left`}</span>
          <span class="muted">· {elapsedText(p.elapsedSecs)} elapsed</span>
        {:else}
          <span class="muted">Starting…</span>
        {/if}
      </div>
      <button class="btn btn-sm btn-ghost" onclick={stop} disabled={stopping}>
        {stopping ? "Stopping…" : "Stop"}
      </button>
    </div>

    <div class="progress" class:indeterminate={!p} style:--value={p ? p.fraction : undefined}><div></div></div>

    {#if p}
      <div class="stats">
        <span><b>{p.done}</b> of {p.total} files</span>
        <span><b>{p.imported}</b> imported</span>
        {#if p.converted > 0}<span><b>{p.converted}</b> converted</span>{/if}
        {#if p.alreadyThere > 0}<span><b>{p.alreadyThere}</b> already there</span>{/if}
        {#if p.skipped > 0}<span class="warn"><b>{p.skipped}</b> skipped</span>{/if}
      </div>

      {#if p.current}
        <div class="current">
          <div class="current-line">
            <span class="action">{ACTION[p.action ?? "importing"]}</span>
            <span class="file" title={p.current}>{p.current}</span>
            {#if p.fileFraction !== null}<span class="file-pct">{Math.round(p.fileFraction * 100)}%</span>{/if}
          </div>
          {#if p.fileFraction !== null}
            <div class="progress thin" style:--value={p.fileFraction}><div></div></div>
          {/if}
          {#if p.action === "converting"}
            <div class="muted detail">
              {p.speed !== null ? `${p.speed.toFixed(1)}× real time` : "Measuring speed…"}{p.encoder ? ` · on your ${p.encoder}` : ""}
            </div>
          {/if}
        </div>
      {/if}
    {/if}
  </section>
{:else if importing.summary}
  <section class="import-panel done" role="status">
    <details>
      <summary>
        <Icon name="alert" size={15} />
        {importing.summary.skipped.length === 1 ? "1 file wasn't imported" : `${importing.summary.skipped.length} files weren't imported`}
      </summary>
      <ul>
        {#each importing.summary.skipped as s (s.file)}
          <li><span class="file">{s.file}</span> <span class="muted">{s.reason}</span></li>
        {/each}
      </ul>
    </details>
    <button class="btn btn-sm btn-ghost btn-icon" onclick={dismissImportSummary} aria-label="Dismiss"><Icon name="close" size={15} /></button>
  </section>
{/if}

<style>
  .import-panel {
    display: grid;
    gap: 0.6rem;
    margin: 0 0 1rem;
    padding: 0.8rem 0.95rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }

  .import-panel.done {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
  }

  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem 1rem;
  }

  .title {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
  }

  .pct {
    color: var(--accent-text, #c9bcff);
    font-weight: 750;
    font-variant-numeric: tabular-nums;
  }

  .times {
    flex: 1;
    font-size: 0.88rem;
    font-variant-numeric: tabular-nums;
  }

  .muted {
    color: var(--text-muted);
  }

  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem 1rem;
    color: var(--text-muted);
    font-size: 0.84rem;
    font-variant-numeric: tabular-nums;
  }

  .stats b {
    color: var(--text);
  }

  .stats .warn,
  .stats .warn b {
    color: var(--warning);
  }

  .current {
    display: grid;
    gap: 0.35rem;
    padding: 0.55rem 0.7rem;
    border-radius: 8px;
    background: var(--surface-2, rgba(255, 255, 255, 0.03));
  }

  .current-line {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    min-width: 0;
    font-size: 0.86rem;
  }

  .action {
    flex: none;
    font-weight: 650;
  }

  .file {
    overflow: hidden;
    min-width: 0;
    color: var(--text-muted);
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.8rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-pct {
    flex: none;
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }

  .progress.thin {
    height: 4px;
  }

  .detail {
    font-size: 0.8rem;
  }

  details summary {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--warning);
    cursor: pointer;
  }

  details ul {
    max-height: 12rem;
    margin: 0.5rem 0 0;
    padding: 0;
    overflow: auto;
    list-style: none;
    font-size: 0.84rem;
  }

  details li {
    padding: 0.2rem 0;
  }

  details li .file {
    display: inline;
    white-space: normal;
    word-break: break-all;
  }
</style>
