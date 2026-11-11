<script lang="ts">
  import { clampThumb, clampTrim, defaultThumb, formatClock, MIN_TRIM_MS, msToX, xToMs } from "../lib/trim";
  import Icon from "./Icon.svelte";

  type Handle = "start" | "end" | "thumb";

  let {
    duration,
    start = $bindable(),
    end = $bindable(),
    thumb = $bindable(null),
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
    /** The thumbnail's frame in ms; null = automatic (shown where it would be taken). */
    thumb?: number | null;
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
  const thumbAt = $derived(thumb === null ? defaultThumb({ start, end }) : clampThumb(thumb, { start, end }));

  function msAt(e: PointerEvent): number {
    const rect = track!.getBoundingClientRect();
    return xToMs(e.clientX - rect.left, duration, rect.width);
  }

  /** Moves one handle, keeping the other in place and the cut at least MIN_TRIM_MS long. */
  function moveHandle(handle: Handle, ms: number) {
    if (handle === "thumb") {
      const at = clampThumb(ms, { start, end });
      if (at === thumbAt && thumb !== null) return;
      thumb = at;
      onhandlemove?.("thumb", at);
      return;
    }
    const next =
      handle === "start"
        ? clampTrim(ms, end, duration, MIN_TRIM_MS, "end")
        : clampTrim(start, ms, duration, MIN_TRIM_MS, "start");
    if (next.start === start && next.end === end) return;
    start = next.start;
    end = next.end;
    // A picked frame stays inside the cut.
    if (thumb !== null) thumb = clampThumb(thumb, next);
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
    const current = handle === "start" ? start : handle === "end" ? end : thumbAt;
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
        next = handle === "thumb" ? start : 0;
        break;
      case "End":
        next = handle === "thumb" ? end : duration;
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

    <div
      class="thumb-mark"
      class:picked={thumb !== null}
      style:left="{pct(thumbAt)}%"
      role="slider"
      tabindex="0"
      aria-label="Thumbnail frame"
      title="Thumbnail: drag to the frame you want"
      aria-orientation="horizontal"
      aria-valuemin={start}
      aria-valuemax={end}
      aria-valuenow={thumbAt}
      aria-valuetext={formatClock(thumbAt)}
      onpointerdown={(e) => grabHandle(e, "thumb")}
      onpointermove={move}
      onpointerup={release}
      onpointercancel={release}
      onkeydown={(e) => onkeydown(e, "thumb")}
    >
      <Icon name="image" size={13} />
    </div>
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
    gap: 0.5rem;
    padding-top: 1.6rem;
    user-select: none;
  }

  /* The thumbnail marker: a tab above the bar with a line down to its frame. */
  .thumb-mark {
    position: absolute;
    top: -1.65rem;
    z-index: 3;
    display: grid;
    place-items: center;
    width: 1.6rem;
    height: 1.25rem;
    margin-left: -0.8rem;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--surface-3, var(--surface-2));
    color: var(--text-muted);
    cursor: ew-resize;
    touch-action: none;
    transition:
      color 0.15s,
      border-color 0.15s;
  }

  .thumb-mark::after {
    content: "";
    position: absolute;
    top: 100%;
    left: 50%;
    width: 0;
    height: calc(3.5rem + 0.3rem);
    border-left: 1px dashed currentColor;
    opacity: 0.7;
    pointer-events: none;
  }

  .thumb-mark.picked,
  .thumb-mark:hover,
  .thumb-mark:focus-visible {
    border-color: #ffc53d;
    color: #ffc53d;
  }

  .track {
    position: relative;
    height: 3.5rem;
    margin: 0 0.9rem;
    border-radius: var(--radius-sm);
    background:
      repeating-linear-gradient(90deg, transparent 0 calc(5% - 1px), rgba(255, 255, 255, 0.045) calc(5% - 1px) 5%),
      linear-gradient(180deg, var(--surface-2), var(--surface));
    box-shadow: inset 0 0 0 1px var(--border);
    cursor: pointer;
    touch-action: none;
  }

  .dim {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgba(5, 4, 10, 0.6);
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
    background: linear-gradient(180deg, rgba(124, 92, 255, 0.24), rgba(124, 92, 255, 0.08));
    box-shadow: 0 0 24px -8px var(--accent-glow);
    pointer-events: none;
  }

  .playhead {
    position: absolute;
    top: -0.35rem;
    bottom: -0.35rem;
    z-index: 2;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: #fff;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }

  /* A knob on top, so the playhead is easy to find. */
  .playhead::before {
    content: "";
    position: absolute;
    top: -4px;
    left: 50%;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #fff;
    box-shadow:
      0 0 0 2px rgba(0, 0, 0, 0.35),
      0 0 10px rgba(255, 255, 255, 0.45);
    transform: translateX(-50%);
  }

  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 0.9rem;
    background: linear-gradient(180deg, #9479ff, #6d4ff2);
    cursor: ew-resize;
    touch-action: none;
    transition: filter 0.15s;
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
    filter: brightness(1.18);
  }

  .handle:focus-visible {
    outline-offset: 1px;
  }

  .grip {
    width: 2px;
    height: 1.2rem;
    border-radius: 1px;
    background: rgba(255, 255, 255, 0.9);
    box-shadow: 3px 0 0 -0.5px rgba(255, 255, 255, 0.35);
  }

  .labels {
    display: flex;
    justify-content: space-between;
    padding: 0 0.9rem;
    color: var(--text-muted);
    font-size: 0.82rem;
    font-variant-numeric: tabular-nums;
  }

  .length {
    padding: 0.05rem 0.55rem;
    border-radius: 999px;
    background: var(--accent-softer);
  }

  .length strong {
    color: var(--text);
    font-weight: 650;
  }
</style>
