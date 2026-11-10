<script lang="ts">
  import { clampTrim, formatClock, MIN_TRIM_MS, msToX, xToMs } from "../lib/trim";

  type Handle = "start" | "end";

  let {
    duration,
    start = $bindable(),
    end = $bindable(),
    position,
    onseek,
    onhandlemove,
  }: {
    /** The clip's length in ms. */
    duration: number;
    /** Trim start in ms. */
    start: number;
    /** Trim end in ms. */
    end: number;
    /** The playhead in ms. */
    position: number;
    /** The user clicked or dragged on the bar to jump to `ms`. */
    onseek: (ms: number) => void;
    /** A handle moved to `ms` (for previewing the frame there). */
    onhandlemove?: (handle: Handle, ms: number) => void;
  } = $props();

  let track = $state<HTMLElement>();
  let dragging = $state<Handle | "playhead" | null>(null);

  const pct = (ms: number) => msToX(ms, duration, 100);

  function msAt(e: PointerEvent): number {
    const rect = track!.getBoundingClientRect();
    return xToMs(e.clientX - rect.left, duration, rect.width);
  }

  /** Moves one handle, keeping the other in place and the cut at least MIN_TRIM_MS long. */
  function moveHandle(handle: Handle, ms: number) {
    const next =
      handle === "start"
        ? clampTrim(ms, end, duration, MIN_TRIM_MS, "end")
        : clampTrim(start, ms, duration, MIN_TRIM_MS, "start");
    if (next.start === start && next.end === end) return;
    start = next.start;
    end = next.end;
    onhandlemove?.(handle, handle === "start" ? next.start : next.end);
  }

  function grabHandle(e: PointerEvent, handle: Handle) {
    if (e.button !== 0) return;
    e.stopPropagation();
    e.preventDefault();
    (e.currentTarget as HTMLElement).focus();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = handle;
  }

  function grabTrack(e: PointerEvent) {
    if (e.button !== 0) return;
    track!.setPointerCapture(e.pointerId);
    dragging = "playhead";
    onseek(msAt(e));
  }

  function move(e: PointerEvent) {
    if (!dragging) return;
    if (dragging === "playhead") onseek(msAt(e));
    else moveHandle(dragging, msAt(e));
  }

  function release() {
    dragging = null;
  }

  function onkeydown(e: KeyboardEvent, handle: Handle) {
    const step = e.shiftKey ? 1000 : 100;
    const current = handle === "start" ? start : end;
    let next: number;
    switch (e.key) {
      case "ArrowLeft":
      case "ArrowDown":
        next = current - step;
        break;
      case "ArrowRight":
      case "ArrowUp":
        next = current + step;
        break;
      case "PageDown":
        next = current - 1000;
        break;
      case "PageUp":
        next = current + 1000;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = duration;
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
    moveHandle(handle, next);
  }
</script>

<div class="trimbar">
  <!-- The bar itself is a pointer target for seeking; keyboard users seek with the handles and
       the player controls, so it has no role of its own. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="track"
    bind:this={track}
    onpointerdown={grabTrack}
    onpointermove={move}
    onpointerup={release}
    onpointercancel={release}
  >
    <div class="dim" style:left="0" style:width="{pct(start)}%"></div>
    <div class="dim" style:left="{pct(end)}%" style:right="0"></div>
    <div class="selection" style:left="{pct(start)}%" style:width="{pct(end) - pct(start)}%"></div>
    <div class="playhead" style:left="{pct(position)}%"></div>

    {#each [["start", start], ["end", end]] as const as [handle, value] (handle)}
      <div
        class="handle {handle}"
        style:left="{pct(value)}%"
        role="slider"
        tabindex="0"
        aria-label={handle === "start" ? "Trim start" : "Trim end"}
        aria-orientation="horizontal"
        aria-valuemin={handle === "start" ? 0 : Math.min(start + MIN_TRIM_MS, duration)}
        aria-valuemax={handle === "start" ? Math.max(0, end - MIN_TRIM_MS) : duration}
        aria-valuenow={value}
        aria-valuetext={formatClock(value)}
        onpointerdown={(e) => grabHandle(e, handle)}
        onpointermove={move}
        onpointerup={release}
        onpointercancel={release}
        onkeydown={(e) => onkeydown(e, handle)}
      >
        <span class="grip"></span>
      </div>
    {/each}
  </div>

  <div class="labels">
    <span>{formatClock(start)}</span>
    <span class="length">Clip length <strong>{formatClock(end - start)}</strong></span>
    <span>{formatClock(end)}</span>
  </div>
</div>

<style>
  .trimbar {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    user-select: none;
  }

  .track {
    position: relative;
    height: 3.25rem;
    margin: 0 0.9rem;
    border-radius: var(--radius-sm);
    background:
      repeating-linear-gradient(90deg, transparent 0 calc(10% - 1px), rgba(255, 255, 255, 0.05) calc(10% - 1px) 10%),
      var(--surface-2);
    cursor: pointer;
    touch-action: none;
  }

  .dim {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgba(5, 6, 9, 0.55);
    pointer-events: none;
  }

  .dim:first-child {
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
  }

  .selection {
    position: absolute;
    top: 0;
    bottom: 0;
    border-top: 2px solid var(--accent);
    border-bottom: 2px solid var(--accent);
    background: var(--accent-soft);
    pointer-events: none;
  }

  .playhead {
    position: absolute;
    top: -0.3rem;
    bottom: -0.3rem;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: #fff;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.4);
    pointer-events: none;
  }

  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 0.9rem;
    background: var(--accent);
    cursor: ew-resize;
    touch-action: none;
  }

  .handle.start {
    margin-left: -0.9rem;
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
  }

  .handle.end {
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  }

  .handle:hover,
  .handle:focus-visible {
    background: var(--accent-hover);
  }

  .handle:focus-visible {
    outline-offset: 1px;
  }

  .grip {
    width: 2px;
    height: 1.1rem;
    border-radius: 1px;
    background: rgba(255, 255, 255, 0.85);
  }

  .labels {
    display: flex;
    justify-content: space-between;
    padding: 0 0.9rem;
    color: var(--text-muted);
    font-size: 0.82rem;
    font-variant-numeric: tabular-nums;
  }

  .length strong {
    color: var(--text);
    font-weight: 650;
  }
</style>
