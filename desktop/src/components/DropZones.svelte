<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { checkDropped } from "../lib/imports.svelte";
  import { dropPoint, isNetworkAddress, onWindows } from "../lib/dropPoint";
  import Icon from "./Icon.svelte";

  // Files dragged over the window: two zones, one adds them to the library where they are, the
  // other copies them into the clips folder first. Dropping anywhere else does nothing. Files from
  // a network share can only be copied (their mounted path doesn't outlast the session), so a
  // drop with any of them shows the copy zone alone.

  type Zone = "keep" | "copy";

  let shown = $state(false);
  let zone = $state<Zone | null>(null);
  let count = $state(0);
  let network = $state(false);

  function zoneAt(pos: { x: number; y: number }): Zone | null {
    const { x, y } = dropPoint(pos, onWindows, window.devicePixelRatio);
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop-zone]");
    return (el?.dataset.dropZone as Zone | undefined) ?? null;
  }

  onMount(() => {
    let stop: (() => void) | null = null;
    let gone = false;
    getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        switch (payload.type) {
          case "enter":
            shown = true;
            count = payload.paths.length;
            network = payload.paths.some(isNetworkAddress);
            zone = zoneAt(payload.position);
            break;
          case "over":
            shown = true;
            zone = zoneAt(payload.position);
            break;
          case "drop": {
            const target = zoneAt(payload.position);
            shown = false;
            zone = null;
            const copy = target === "copy" || payload.paths.some(isNetworkAddress);
            if (target && payload.paths.length > 0) void checkDropped(payload.paths, copy);
            break;
          }
          case "leave":
            shown = false;
            zone = null;
            break;
        }
      })
      .then((unlisten) => {
        if (gone) unlisten();
        else stop = unlisten;
      })
      .catch(() => {});
    return () => {
      gone = true;
      stop?.();
    };
  });

  const what = $derived(count === 1 ? "this file" : count > 1 ? `these ${count} items` : "these files");
</script>

{#if shown}
  <div class="drop-overlay" aria-hidden="true">
    <p class="heading">Drop to import {what}</p>
    <div class="zones" class:single={network}>
      {#if !network}
        <div class="zone" class:active={zone === "keep"} data-drop-zone="keep">
          <div class="badge"><Icon name="link" size={26} /></div>
          <strong>Add to library</strong>
          <span>Keep the files where they are. Klipt just adds them to your clips.</span>
        </div>
      {/if}
      <div class="zone" class:active={zone === "copy"} data-drop-zone="copy">
        <div class="badge"><Icon name="folder" size={26} /></div>
        <strong>Copy to clips folder</strong>
        {#if network}
          <span>These are on a network share, so Klipt copies them. Your clips still play when the share is offline.</span>
        {:else}
          <span>Klipt keeps its own copy, so you can move or delete the originals.</span>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
    padding: 2.2rem;
    background: rgba(11, 10, 16, 0.78);
    backdrop-filter: blur(6px);
    animation: fade 0.12s var(--ease-out);
  }

  .heading {
    margin: 0;
    text-align: center;
    font-family: var(--font-display);
    font-size: 1.35rem;
    font-weight: 650;
    color: var(--text);
  }

  .zones {
    display: grid;
    flex: 1;
    grid-template-columns: 1fr 1fr;
    gap: 1.1rem;
  }

  .zones.single {
    grid-template-columns: 1fr;
  }

  .zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.6rem;
    padding: 1.5rem;
    border: 2px dashed var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--surface);
    text-align: center;
    transition:
      border-color 0.12s,
      background 0.12s,
      box-shadow 0.12s,
      transform 0.16s var(--ease-out);
  }

  .zone strong {
    font-size: 1.15rem;
    color: var(--text);
  }

  .zone span {
    max-width: 18rem;
    color: var(--text-muted);
    line-height: 1.4;
  }

  .badge {
    display: grid;
    place-items: center;
    width: 3.6rem;
    height: 3.6rem;
    margin-bottom: 0.2rem;
    border-radius: 50%;
    background: var(--surface-3);
    color: var(--text-muted);
    transition:
      background 0.12s,
      color 0.12s;
  }

  .zone.active {
    border-style: solid;
    border-color: var(--accent);
    background: var(--accent-softer);
    box-shadow: 0 0 0 4px var(--accent-soft), 0 0 40px var(--accent-glow);
    transform: scale(1.015);
  }

  .zone.active .badge {
    background: var(--accent);
    color: #fff;
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .drop-overlay,
    .zone {
      animation: none;
      transition: none;
    }
  }
</style>
