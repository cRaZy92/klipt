<script lang="ts">
  import { clampPopupLeft, clampThumb, clampTrim, defaultThumb, formatClock, MIN_TRIM_MS } from "../lib/trim";
  import { fullView, inView, panView, viewMs, viewPct, viewSpan, type TimeView } from "../lib/timelineView";
  import { edgeScroller, edgeSpeed } from "../lib/edgeScroll";
  import { canRemove, clampToKept, deleteSplit, keptRanges, moveSplit, toggle, type Piece } from "../lib/cuts";
  import { frameUrl, previewIndex, tiles } from "../lib/filmstrip";
  import Icon from "./Icon.svelte";

  /**
   * Moves an element to the end of <body> while it's shown, so no container can cover or clip it
   * (the lanes clip their overflow and the video lane is its own stacking layer; WebKitGTK also
   * clips fixed elements inside rounded overflow containers).
   */
  function toBody(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  type Handle = "start" | "end" | "thumb";
  type Moved = Handle | "split";

  let {
    duration,
    start = $bindable(),
    end = $bindable(),
    thumb = $bindable(null),
    pieces = $bindable(),
    position,
    strip = null,
    onseek,
    onhandlemove,
    onscrubstart,
    onscrubend,
    locked = false,
    view = null,
    onview,
  }: {
    /** The clip's length in ms. */
    duration: number;
    /** Trim start in ms. */
    start: number;
    /** Trim end in ms. */
    end: number;
    /** The thumbnail's frame in ms; null = automatic (shown where it would be taken). */
    thumb?: number | null;
    /** The timeline split into kept and removed pieces (covering the whole clip). */
    pieces: Piece[];
    /** The playhead in ms. */
    position: number;
    /** The clip's filmstrip folder and how many of its pictures exist so far. */
    strip?: { dir: string; ready: number } | null;
    /** The user clicked or dragged on the bar to jump to `ms`. */
    onseek: (ms: number) => void;
    /** A handle moved to `ms` (for previewing the frame there). */
    onhandlemove?: (handle: Moved, ms: number) => void;
    /** The pointer went down on the bar, a handle or a split (keyboard nudges don't count). */
    onscrubstart?: () => void;
    /** That pointer was released, or the browser took it away (pointercancel). */
    onscrubend?: () => void;
    /** The edit can't change now (the clip is uploading or exporting): only the playhead moves. */
    locked?: boolean;
    /** The stretch of the clip the timeline shows (zoomed in); null = the whole clip. */
    view?: TimeView | null;
    /** A drag held at an edge moves the zoomed-in view along (auto-scroll). */
    onview?: (view: TimeView) => void;
  } = $props();

  const shownView = $derived(view ?? fullView(duration));

  let track = $state<HTMLElement>();
  /** A handle, the playhead, or the split after piece `n`. */
  let trackWidth = $state(0);
  let trackHeight = $state(0);
  /** Tiles whose picture failed to load stay hidden. */
  let brokenTiles = $state(new Set<number>());
  const filmTiles = $derived(
    strip
      ? tiles(trackWidth, (trackHeight * 16) / 9, duration, shownView).flatMap((t) => {
          const index = previewIndex(t.ms, strip.ready);
          return index === null || brokenTiles.has(index) ? [] : [{ ...t, index, src: frameUrl(strip.dir, index) }];
        })
      : [],
  );
  let dragging = $state<Handle | "playhead" | number | null>(null);

  /** The thumbnail marker's preview shows while it is hovered, focused or dragged. */
  const POP_WIDTH = 120;
  let thumbEl = $state<HTMLElement>();
  let thumbHover = $state(false);
  let thumbFocus = $state(false);
  const thumbActive = $derived(thumbHover || thumbFocus || dragging === "thumb");
  const thumbPicture = $derived.by(() => {
    if (!strip) return null;
    const index = previewIndex(thumbAt, strip.ready);
    return index === null ? null : frameUrl(strip.dir, index);
  });
  /**
   * Where the preview goes, in viewport px: fixed, so the lanes' overflow doesn't clip it. Centred
   * over the marker and kept inside the track's width.
   */
  let pop = $state({ left: 0, top: 0 });
  $effect(() => {
    if (!thumbActive || !thumbEl || !track) return;
    // Re-place it when the marker moves or the track resizes.
    void thumbAt;
    void trackWidth;
    const mark = thumbEl.getBoundingClientRect();
    const bar = track.getBoundingClientRect();
    pop = {
      left: clampPopupLeft(mark.left + mark.width / 2, POP_WIDTH, bar.left, bar.right),
      top: mark.top - 6,
    };
  });

  /** Where `ms` is on the bar, in percent (outside 0–100 when zoomed in and off screen). */
  const pct = (ms: number) => viewPct(ms, shownView);
  /** The part of a stretch that is on screen; null when none is. */
  const part = (from: number, to: number) => viewSpan(from, to, shownView);
  /** Whether a marker at `ms` is on screen. */
  const onScreen = (ms: number) => inView(ms, shownView);
  const dimLeft = $derived(part(0, start));
  const dimRight = $derived(part(end, duration));
  const selection = $derived(part(start, end));
  const keptParts = $derived(keptRanges(pieces, { start, end }));
  const thumbAt = $derived(
    clampToKept(thumb === null ? defaultThumb({ start, end }) : clampThumb(thumb, { start, end }), keptParts),
  );
  /** The pieces as far as they show inside the trim (the rest is hidden, not forgotten). */
  const shown = $derived(
    pieces
      .map((p, i) => ({ ...p, i, from: Math.max(p.start, start), to: Math.min(p.end, end) }))
      .filter((p) => p.to > p.from),
  );
  /** Split lines inside the trim and on screen: the boundary after piece `i`. */
  const splits = $derived(
    pieces
      .slice(0, -1)
      .map((p, i) => ({ i, at: p.end }))
      .filter((s) => s.at > start && s.at < end && onScreen(s.at)),
  );

  /** The pointer of the drag in progress, as last seen: auto-scroll moves the drag on with it. */
  let lastPointer: PointerEvent | null = null;
  /** Held at an edge while dragging: the view moves along and the handle (or playhead) with it. */
  const autoScroll = edgeScroller((dt) => {
    if (dragging === null || !lastPointer || !track || !onview) return;
    const rect = track.getBoundingClientRect();
    const speed = edgeSpeed(lastPointer.clientX - rect.left, rect.width);
    if (!speed) return;
    const v = shownView;
    const next = panView(v, duration, (speed * dt * (v.to - v.from)) / 1000);
    if (next === v) return;
    onview(next);
    move(lastPointer);
  });

  $effect(() => () => autoScroll.stop());

  /** A drag starts: watch for it being held at an edge. */
  function startDrag(e: PointerEvent) {
    lastPointer = e;
    autoScroll.start();
  }

  function moveSplitTo(i: number, ms: number) {
    if (locked) return;
    const next = moveSplit(pieces, i, ms);
    if (next === pieces) return;
    pieces = next;
    onhandlemove?.("split", next[i].end);
  }

  function grabSplit(e: PointerEvent, i: number) {
    // Locked: the pointer goes on to the bar, which moves the playhead.
    if (e.button !== 0 || locked) return;
    e.stopPropagation();
    e.preventDefault();
    (e.currentTarget as HTMLElement).focus();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = i;
    startDrag(e);
    onscrubstart?.();
  }

  function onsplitkey(e: KeyboardEvent, i: number) {
    const step = e.shiftKey ? 1000 : 100;
    const at = pieces[i].end;
    if (locked) return;
    if (e.key === "Delete" || e.key === "Backspace") pieces = deleteSplit(pieces, i);
    else if (e.key === "ArrowLeft" || e.key === "ArrowDown") moveSplitTo(i, at - step);
    else if (e.key === "ArrowRight" || e.key === "ArrowUp") moveSplitTo(i, at + step);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  /** Buttons on the bar must not start a seek on it. */
  const stop = (e: PointerEvent) => e.stopPropagation();

  function msAt(e: PointerEvent): number {
    const rect = track!.getBoundingClientRect();
    return viewMs(e.clientX - rect.left, rect.width, shownView);
  }

  /** Moves one handle, keeping the other in place and the cut at least MIN_TRIM_MS long. */
  function moveHandle(handle: Handle, ms: number) {
    if (locked) return;
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
    // Locked: the pointer goes on to the bar, which moves the playhead.
    if (e.button !== 0 || locked) return;
    e.stopPropagation();
    e.preventDefault();
    (e.currentTarget as HTMLElement).focus();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = handle;
    startDrag(e);
    onscrubstart?.();
  }

  function grabTrack(e: PointerEvent) {
    if (e.button !== 0) return;
    // Like the handles: no text selection starts, so dragging doesn't select the page around.
    e.preventDefault();
    track!.setPointerCapture(e.pointerId);
    dragging = "playhead";
    startDrag(e);
    onscrubstart?.();
    onseek(msAt(e));
  }

  function move(e: PointerEvent) {
    if (dragging === null) return;
    lastPointer = e;
    if (dragging === "playhead") onseek(msAt(e));
    else if (typeof dragging === "number") moveSplitTo(dragging, msAt(e));
    else moveHandle(dragging, msAt(e));
  }

  function release() {
    // A handle's pointerup also bubbles to the bar: end the scrub once.
    if (dragging === null) return;
    dragging = null;
    lastPointer = null;
    autoScroll.stop();
    onscrubend?.();
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

<div class="trimbar" class:locked>
  <!-- The bar itself is a pointer target for seeking; keyboard users seek with the handles and
       the player controls, so it has no role of its own. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="track"
    bind:this={track}
    bind:clientWidth={trackWidth}
    bind:clientHeight={trackHeight}
    onpointerdown={grabTrack}
    onpointermove={move}
    onpointerup={release}
    onpointercancel={release}
    onlostpointercapture={release}
  >
    {#if filmTiles.length}
      <div class="film" aria-hidden="true">
        {#each filmTiles as t (t.i)}
          <img
            src={t.src}
            alt=""
            loading="lazy"
            decoding="async"
            style:left="{t.leftPct}%"
            style:width="{t.widthPct}%"
            onerror={() => (brokenTiles = new Set(brokenTiles).add(t.index))}
          />
        {/each}
      </div>
    {/if}
    {#if dimLeft}
      <div class="dim" class:left={dimLeft.left === 0 && pct(0) >= 0} style:left="{dimLeft.left}%" style:width="{dimLeft.width}%"></div>
    {/if}
    {#if dimRight}
      <div class="dim" class:right={pct(duration) <= 100} style:left="{dimRight.left}%" style:width="{dimRight.width}%"></div>
    {/if}
    {#if selection}
      <div class="selection" style:left="{selection.left}%" style:width="{selection.width}%"></div>
    {/if}
    {#each shown as p (p.i)}
      {@const visible = part(p.from, p.to)}
      {#if p.removed && visible}
        <div
          class="cut"
          class:open-left={pct(p.from) < 0}
          class:open-right={pct(p.to) > 100}
          style:left="{visible.left}%"
          style:width="{visible.width}%"
        ></div>
      {/if}
      {#if shown.length > 1 && visible}
        {@const allowed = canRemove(pieces, p.i, { start, end })}
        <button
          class="piece-toggle"
          class:removed={p.removed}
          style:left="{visible.left + visible.width / 2}%"
          disabled={!allowed || locked}
          aria-label={p.removed ? "Keep this part" : "Remove this part"}
          title={!allowed ? "Keep at least part of the clip" : p.removed ? "Keep this part" : "Remove this part"}
          onpointerdown={stop}
          onclick={() => (pieces = toggle(pieces, p.i))}
        >
          {#if p.removed}<Icon name="retry" size={12} />{:else}<Icon name="scissors" size={12} />{/if}
        </button>
      {/if}
    {/each}
    {#each splits as s (s.i)}
      <div
        class="split"
        style:left="{pct(s.at)}%"
        role="slider"
        tabindex="0"
        aria-disabled={locked}
        aria-label="Split"
        aria-orientation="horizontal"
        aria-valuemin={0}
        aria-valuemax={duration}
        aria-valuenow={s.at}
        aria-valuetext={formatClock(s.at)}
        title="Split: drag to move, Delete to remove"
        onpointerdown={(e) => grabSplit(e, s.i)}
        onpointermove={move}
        onpointerup={release}
        onpointercancel={release}
        onlostpointercapture={release}
        onkeydown={(e) => onsplitkey(e, s.i)}
      >
        <button
          class="split-remove"
          aria-label="Remove split"
          title="Remove split"
          tabindex="-1"
          disabled={locked}
          onpointerdown={stop}
          onclick={() => (pieces = deleteSplit(pieces, s.i))}
        >
          <Icon name="close" size={10} />
        </button>
      </div>
    {/each}
    {#if onScreen(position)}
      <div class="playhead" style:left="{pct(position)}%"></div>
    {/if}

    {#each [["start", start], ["end", end]] as const as [handle, value] (handle)}
      <div
        class="handle {handle}"
        class:off={!onScreen(value)}
        style:left="{pct(value)}%"
        role="slider"
        tabindex="0"
        aria-disabled={locked}
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
        onlostpointercapture={release}
        onkeydown={(e) => onkeydown(e, handle)}
      >
        <span class="grip"></span>
      </div>
    {/each}

    <div
      class="thumb-mark"
      class:picked={thumb !== null}
      class:active={thumbActive}
      class:off={!onScreen(thumbAt)}
      bind:this={thumbEl}
      onpointerenter={() => (thumbHover = true)}
      onpointerleave={() => (thumbHover = false)}
      onfocus={() => (thumbFocus = true)}
      onblur={() => (thumbFocus = false)}
      style:left="{pct(thumbAt)}%"
      role="slider"
      tabindex="0"
      aria-disabled={locked}
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
      onlostpointercapture={release}
      onkeydown={(e) => onkeydown(e, "thumb")}
    >
      <Icon name="image" size={13} />
    </div>
    {#if thumbActive && onScreen(thumbAt)}
      <div class="thumb-pop" use:toBody style:left="{pop.left}px" style:top="{pop.top}px" aria-hidden="true">
        {#if thumbPicture}<img src={thumbPicture} alt="" decoding="async" />{/if}
        <span>Thumbnail · {formatClock(thumbAt)}</span>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Locked: the handles, splits and thumbnail marker can't be grabbed (the bar still seeks). */
  .locked .handle,
  .locked .split,
  .locked .thumb-mark {
    cursor: not-allowed;
  }

  /* Room above the track for the thumbnail marker's tab. */
  .trimbar {
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
  .thumb-mark.active,
  .thumb-mark:hover,
  .thumb-mark:focus-visible {
    border-color: var(--warning);
    color: var(--warning);
  }

  /* The preview above the marker: fixed (the lanes clip their overflow), never takes the pointer. */
  .thumb-pop {
    position: fixed;
    z-index: 20;
    width: 120px;
    overflow: hidden;
    border: 1px solid var(--warning);
    border-radius: 6px;
    background: var(--surface-3, var(--surface-2));
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
    transform: translateY(-100%);
    pointer-events: none;
  }

  .thumb-pop img {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
  }

  .thumb-pop span {
    display: block;
    padding: 0.15rem 0.3rem;
    color: var(--warning);
    font-size: 0.68rem;
    font-variant-numeric: tabular-nums;
    text-align: center;
    white-space: nowrap;
  }

  /* A removed part: red hatching over the bar. */
  .cut {
    position: absolute;
    top: 0;
    bottom: 0;
    background: repeating-linear-gradient(
      135deg,
      rgba(255, 93, 108, 0.32) 0 6px,
      rgba(255, 93, 108, 0.1) 6px 12px
    );
    box-shadow:
      inset 2px 0 0 var(--danger),
      inset -2px 0 0 var(--danger);
    pointer-events: none;
  }

  /* A removed part running off screen (zoomed in) has no edge line where the screen cuts it. */
  .cut.open-left {
    box-shadow: inset -2px 0 0 var(--danger);
  }

  .cut.open-right {
    box-shadow: inset 2px 0 0 var(--danger);
  }

  .cut.open-left.open-right {
    box-shadow: none;
  }

  /* Remove / keep a part: shown while the bar is hovered or the button has focus. */
  .piece-toggle {
    position: absolute;
    top: 4px;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 1.35rem;
    height: 1.35rem;
    margin-left: -0.675rem;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--surface-3);
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .track:hover .piece-toggle,
  .piece-toggle:focus-visible {
    opacity: 1;
  }

  .piece-toggle:hover:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger-text);
  }

  .piece-toggle.removed:hover {
    border-color: var(--success);
    color: var(--success);
  }

  .piece-toggle:disabled {
    cursor: not-allowed;
    opacity: 0;
  }

  .track:hover .piece-toggle:disabled {
    opacity: 0.4;
  }

  /* A split line: dashed, draggable, with a remove button underneath. */
  .split {
    position: absolute;
    top: -6px;
    bottom: -6px;
    z-index: 2;
    width: 12px;
    margin-left: -6px;
    cursor: ew-resize;
    touch-action: none;
  }

  .split::before {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 5px;
    border-left: 2px dashed var(--warning);
  }

  .split:focus-visible {
    outline: none;
  }

  .split:focus-visible::before {
    border-left-style: solid;
  }

  .split-remove {
    position: absolute;
    bottom: -1.1rem;
    left: 50%;
    display: grid;
    place-items: center;
    width: 1rem;
    height: 1rem;
    margin-left: -0.5rem;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    background: var(--surface-3);
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .split:hover .split-remove,
  .split:focus-visible .split-remove {
    opacity: 1;
  }

  .split-remove:hover {
    border-color: var(--danger);
    color: var(--danger-text);
  }

  .track {
    position: relative;
    height: 3.5rem;
    /* TrackLanes insets its lanes as far (--inset), so they line up with this track. */
    margin: 0 0.9rem;
    border-radius: var(--radius-sm);
    background:
      repeating-linear-gradient(90deg, transparent 0 calc(5% - 1px), rgba(255, 255, 255, 0.045) calc(5% - 1px) 5%),
      linear-gradient(180deg, var(--surface-2), var(--surface));
    box-shadow: inset 0 0 0 1px var(--border);
    cursor: pointer;
    touch-action: none;
  }

  /* The filmstrip: clipped to the track, behind everything else, never changes its size. */
  .film {
    position: absolute;
    inset: 0;
    overflow: hidden;
    border-radius: inherit;
    pointer-events: none;
  }

  .film img {
    position: absolute;
    top: 0;
    height: 100%;
    object-fit: cover;
    pointer-events: none;
  }

  .dim {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgba(5, 4, 10, 0.6);
    pointer-events: none;
  }

  .dim.left {
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
  }

  .dim.right {
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  }

  /* Zoomed in: a handle or the thumbnail marker off screen keeps its place (and focus) unseen. */
  .handle.off,
  .thumb-mark.off {
    visibility: hidden;
    pointer-events: none;
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
</style>
