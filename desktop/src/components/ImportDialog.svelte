<script lang="ts">
  import { closeImport, confirmImport, importing } from "../lib/imports.svelte";
  import { etaText, sizeText, videoLengthText } from "../lib/importSummary";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  // Step 1 and 2 of an import: checking the folder (with a live count), then what it will do and
  // how long it should take, for the user to confirm. Long conversions get a clear warning.

  const p = $derived(importing.preview);
  const toImport = $derived(p ? p.inPlace + p.remux + p.reencode : 0);
  /** Over ~10 minutes, or any re-encode on the CPU: worth a warning. */
  const long = $derived(!!p && (p.estimateSecs > 600 || (p.reencode > 0 && p.encoder === "CPU")));
  const folderName = $derived(importing.folder?.split(/[\\/]/).filter(Boolean).pop() ?? "");
</script>

{#if importing.phase === "checking"}
  <Modal title="Checking the folder…" onclose={closeImport}>
    <p class="lead">
      Looking at every video and picture in <strong>{folderName}</strong> to see what importing it takes.
    </p>
    <div class="progress" class:indeterminate={importing.found === 0} style:--value={importing.found > 0 ? importing.checked / importing.found : undefined}>
      <div></div>
    </div>
    <p class="muted count">
      {importing.found > 0 ? `${importing.checked} of ${importing.found} files checked` : "Finding files…"}
    </p>
    {#snippet footer()}
      <button class="btn" onclick={closeImport}>Cancel</button>
    {/snippet}
  </Modal>
{:else if importing.phase === "confirm" && p}
  <Modal title="Import {toImport === 1 ? '1 file' : `${toImport} files`}?" onclose={closeImport} width="32rem">
    <p class="lead">Found {p.files === 1 ? "1 file" : `${p.files} files`} in <strong title={p.folder}>{folderName}</strong>:</p>
    <ul class="breakdown">
      {#if p.inPlace > 0}
        <li><Icon name="check" size={16} /><span><b>{p.inPlace}</b> ready to use as they are <em>(instant, nothing is copied)</em></span></li>
      {/if}
      {#if p.remux > 0}
        <li><Icon name="folder" size={16} /><span><b>{p.remux}</b> need a quick container change <em>(fast copy)</em></span></li>
      {/if}
      {#if p.reencode > 0}
        <li class="slow">
          <Icon name="film" size={16} />
          <span>
            <b>{p.reencode}</b> need converting to H.264
            <em>({videoLengthText(p.reencodeVideoMs)} of video{p.encoder ? `, on your ${p.encoder}` : ""})</em>
          </span>
        </li>
      {/if}
      {#if p.alreadyThere > 0}
        <li class="muted"><Icon name="check" size={16} /><span><b>{p.alreadyThere}</b> already in Klipt <em>(skipped)</em></span></li>
      {/if}
      {#if p.unsupported > 0}
        <li class="muted"><Icon name="close" size={16} /><span><b>{p.unsupported}</b> can't be imported <em>(not a video or picture Klipt can read)</em></span></li>
      {/if}
    </ul>

    <div class="estimate">
      <div><span class="muted">Estimated time</span><strong>{etaText(p.estimateSecs)}</strong></div>
      {#if p.copyBytes > 0}
        <div><span class="muted">Space for copies</span><strong>about {sizeText(p.copyBytes)}</strong></div>
      {/if}
    </div>

    {#if p.noEncoder}
      <p class="warn">
        <Icon name="alert" size={16} />
        <span>No H.264 encoder works on this PC, so the {p.reencode} files that need converting will be skipped.</span>
      </p>
    {:else if long}
      <p class="warn">
        <Icon name="alert" size={16} />
        <span>
          <strong>This can take a long time.</strong>
          Converting keeps your {p.encoder === "CPU" ? "processor" : "graphics card"} busy, so games may run slower until it's done.
          You can keep using Klipt meanwhile and press <em>Stop</em> at any time. Clips imported so far stay.
        </span>
      </p>
    {/if}
    {#if p.copyBytes > 0}
      <p class="muted small">
        Copies go to <span class="path" title={p.copiesDir}>{p.copiesDir}</span>. Your original files are never changed.
      </p>
    {:else}
      <p class="muted small">Your original files are never changed or moved.</p>
    {/if}

    {#snippet footer()}
      <button class="btn" onclick={closeImport}>Cancel</button>
      <button class="btn btn-primary" onclick={confirmImport}>
        {long ? "Start anyway" : "Import"}
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  .lead {
    margin: 0 0 0.9rem;
  }

  .count {
    margin: 0.6rem 0 0;
    font-size: 0.86rem;
    font-variant-numeric: tabular-nums;
  }

  .muted {
    color: var(--text-muted);
  }

  .small {
    margin: 0.8rem 0 0;
    font-size: 0.82rem;
  }

  .breakdown {
    display: grid;
    gap: 0.45rem;
    margin: 0 0 1rem;
    padding: 0;
    list-style: none;
  }

  .breakdown li {
    display: flex;
    align-items: flex-start;
    gap: 0.55rem;
    line-height: 1.35;
  }

  .breakdown li :global(svg) {
    flex: none;
    margin-top: 0.1rem;
    color: var(--accent-text, #c9bcff);
  }

  .breakdown li.slow :global(svg) {
    color: var(--warning);
  }

  .breakdown li.muted :global(svg) {
    color: var(--text-muted);
  }

  .breakdown em {
    color: var(--text-muted);
    font-style: normal;
  }

  .estimate {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
  }

  .estimate div {
    display: grid;
    flex: 1 1 10rem;
    gap: 0.15rem;
    padding: 0.65rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2, rgba(255, 255, 255, 0.03));
  }

  .estimate .muted {
    font-size: 0.78rem;
  }

  .estimate strong {
    font-size: 1.05rem;
  }

  .warn {
    display: flex;
    gap: 0.55rem;
    margin: 0.9rem 0 0;
    padding: 0.7rem 0.8rem;
    border: 1px solid rgba(255, 181, 71, 0.35);
    border-radius: var(--radius);
    background: var(--warning-soft);
    font-size: 0.88rem;
    line-height: 1.4;
  }

  .warn :global(svg) {
    flex: none;
    margin-top: 0.1rem;
    color: var(--warning);
  }

  .warn em {
    font-style: normal;
    font-weight: 650;
  }

  .path {
    font-family: var(--font-mono, ui-monospace, monospace);
    word-break: break-all;
  }
</style>
