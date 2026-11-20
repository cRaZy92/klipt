<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Piece } from "../lib/cuts";
  import { laneShading, rulerTicks } from "../lib/lanes";
  import { msToX, xToMs, type TrimRange } from "../lib/trim";
  import { peaks as bucketPeaks } from "../lib/waveform";
  import type { SubtitleRange, ZoomRange } from "../lib/api";
  import { moveZoom, rampMs, resizeZoom } from "../lib/zoom";
  import { moveSubtitle, resizeSubtitle } from "../lib/subtitles";
  import Icon from "./Icon.svelte";

  interface LaneTrack {
    /** The audio engine's key: "game", "mic", "voice" or "track-<index>". */
    key: string;
    name: string;
    /** The track's waveform (0–1 per bucket); null while it isn't loaded. */
    peaks: Float32Array | null;
    /** 0–100. */
    volume: number;
    muted: boolean;
  }

  let {
    duration,
    trim,
    pieces,
    position,
    tracks,
    blur,
    locked = false,
    onvolume,
    onmute,
    onblur,
    onseek,
    onscrubstart,
    onscrubend,
    zooms,
    selectedZoom,
    onzoomselect,
    onzoomchange,
    onzoomgesture,
    onzoomremove,
    subtitles,
    selectedSubtitle,
    onsubtitleselect,
    onsubtitlechange,
    onsubtitlegesture,
    onsubtitleremove,
    onsubtitleedit,
    onlanedown,
    video,
  }: {
    /** The clip's length in ms. */
    duration: number;
    trim: TrimRange;
    /** The timeline's kept and removed pieces. */
    pieces: Piece[];
    /** The playhead in ms. */
    position: number;
    /** One lane per audio track, top to bottom. */
    tracks: LaneTrack[];
    /** "Blur video" is on. */
    blur: boolean;
    /** The edit can't change now (the clip is uploading or exporting): only the playhead moves. */
    locked?: boolean;
    onvolume: (key: string, volume: number) => void;
    onmute: (key: string) => void;
    onblur: (blur: boolean) => void;
    /** The user clicked or dragged on a lane to jump to `ms`. */
    onseek: (ms: number) => void;
    /** The pointer went down on a lane. */
    onscrubstart?: () => void;
    /** That pointer was released, or the browser took it away. */
    onscrubend?: () => void;
    /** The zooms, sorted, in ms into the file. */
    zooms: ZoomRange[];
    /** The picked zoom's index, or null. */
    selectedZoom: number | null;
    onzoomselect: (i: number | null) => void;
    onzoomchange: (zooms: ZoomRange[]) => void;
    /** A drag of a zoom started (true) or ended (false): one drag is one undo step. */
    onzoomgesture: (active: boolean) => void;
    onzoomremove: (i: number) => void;
    /** The subtitles, sorted, in ms into the file. */
    subtitles: SubtitleRange[];
    /** The picked subtitle's index, or null. */
    selectedSubtitle: number | null;
    onsubtitleselect: (i: number | null) => void;
    onsubtitlechange: (subtitles: SubtitleRange[]) => void;
    /** A drag of a subtitle started (true) or ended (false): one drag is one undo step. */
    onsubtitlegesture: (active: boolean) => void;
    onsubtitleremove: (i: number) => void;
    /** Opens subtitle `i`'s text box. */
    onsubtitleedit: (i: number) => void;
    /**
     * A press on the Zoom or Subtitles lane, before anything else happens: an open text box closes
     * (which can remove a subtitle and shift the others' indices).
     */
    onlanedown?: () => void;
    /** The video lane: the TrimBar. */
    video: Snippet;
  } = $props();

  const ticks = $derived(rulerTicks(duration));
  const shading = $derived(laneShading(duration, trim, pieces));
  /** Ruler, video, zoom, subtitles, one row per track: the playhead spans all but the first two. */
  const rows = $derived(["auto", "auto", "auto", "auto", ...tracks.map(() => "auto")].join(" "));

  const level = (v: number) => (v === 0 ? "Off" : `${v}%`);

  /** The lane being dragged on (its pointer is captured). */
  let scrubbing: EventTarget | null = null;

  function msAt(e: PointerEvent, area: HTMLElement): number {
    const rect = area.getBoundingClientRect();
    return xToMs(e.clientX - rect.left, duration, rect.width);
  }

  // Like the video lane's track: the pointer seeks, and a drag scrubs.
  function grab(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const area = e.currentTarget as HTMLElement;
    area.setPointerCapture(e.pointerId);
    scrubbing = area;
    onscrubstart?.();
    onseek(msAt(e, area));
  }

  function move(e: PointerEvent) {
    if (scrubbing !== null && scrubbing === e.currentTarget) onseek(msAt(e, e.currentTarget as HTMLElement));
  }

  function release() {
    if (scrubbing === null) return;
    scrubbing = null;
    onscrubend?.();
  }

  /** A zoom being dragged on the Zoom lane: its body (moves it) or one edge (resizes it). */
  let zoomDrag: { i: number; what: "move" | "start" | "end"; offset: number; moved: boolean } | null = $state(null);
  /** How close to a zoom's edge (px) the pointer resizes it instead of moving it. */
  const EDGE_PX = 10;

  function zoomDown(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    onlanedown?.();
    e.preventDefault();
    e.stopPropagation();
    const block = e.currentTarget as HTMLElement;
    const area = block.parentElement as HTMLElement;
    const z = zooms[i];
    onzoomselect(i);
    if (locked) {
      onseek(z.startMs + rampMs(z));
      return;
    }
    const r = block.getBoundingClientRect();
    const what = e.clientX - r.left <= EDGE_PX ? "start" : r.right - e.clientX <= EDGE_PX ? "end" : "move";
    block.setPointerCapture(e.pointerId);
    zoomDrag = { i, what, offset: msAt(e, area) - z.startMs, moved: false };
    onzoomgesture(true);
  }

  function zoomMove(e: PointerEvent) {
    if (!zoomDrag) return;
    const area = (e.currentTarget as HTMLElement).parentElement as HTMLElement;
    const ms = msAt(e, area);
    const { i, what } = zoomDrag;
    const next = what === "move" ? moveZoom(zooms, i, ms - zoomDrag.offset, trim) : resizeZoom(zooms, i, what, ms, trim);
    if (next !== zooms) {
      zoomDrag.moved = true;
      onzoomchange(next);
    }
  }

  function zoomUp() {
    if (!zoomDrag) return;
    const { i, what } = zoomDrag;
    zoomDrag = null;
    onzoomgesture(false);
    // Show the zoom where it is fully in, so its box can be placed (a resized end shows the end).
    const z = zooms[i];
    if (z) onseek(what === "end" ? z.endMs - rampMs(z) : z.startMs + rampMs(z));
  }

  function zoomKey(e: KeyboardEvent, i: number) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      onzoomselect(i);
      onseek(zooms[i].startMs + rampMs(zooms[i]));
    }
  }

  /** A subtitle being dragged on the Subtitles lane: its body (moves it) or one edge (resizes it). */
  let subDrag: { i: number; what: "move" | "start" | "end"; offset: number } | null = $state(null);

  function subDown(e: PointerEvent, index: number) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const block = e.currentTarget as HTMLElement;
    const area = block.parentElement as HTMLElement;
    // Closing an open text box can remove a blank subtitle before this one: find it again.
    const startMs = subtitles[index].startMs;
    onlanedown?.();
    const i = subtitles.findIndex((s) => s.startMs === startMs);
    if (i < 0) return;
    const s = subtitles[i];
    onsubtitleselect(i);
    onseek(s.startMs);
    if (locked) return;
    const r = block.getBoundingClientRect();
    const what = e.clientX - r.left <= EDGE_PX ? "start" : r.right - e.clientX <= EDGE_PX ? "end" : "move";
    block.setPointerCapture(e.pointerId);
    subDrag = { i, what, offset: msAt(e, area) - s.startMs };
    onsubtitlegesture(true);
  }

  function subMove(e: PointerEvent) {
    if (!subDrag) return;
    const area = (e.currentTarget as HTMLElement).parentElement as HTMLElement;
    const ms = msAt(e, area);
    const { i, what } = subDrag;
    const next =
      what === "move" ? moveSubtitle(subtitles, i, ms - subDrag.offset, trim) : resizeSubtitle(subtitles, i, what, ms, trim);
    if (next !== subtitles) onsubtitlechange(next);
  }

  function subUp() {
    if (!subDrag) return;
    const { i, what } = subDrag;
    subDrag = null;
    onsubtitlegesture(false);
    const s = subtitles[i];
    if (s) onseek(what === "end" ? Math.max(s.startMs, s.endMs - 1) : s.startMs);
  }

  function subKey(e: KeyboardEvent, i: number) {
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      onsubtitleselect(i);
      onseek(subtitles[i].startMs);
      if (!locked) onsubtitleedit(i);
    }
  }

  /** Draws `peaks` across the canvas in device pixels, again whenever the canvas is resized. */
  function waveform(canvas: HTMLCanvasElement, initial: Float32Array | null) {
    let current = initial;
    function draw() {
      const dpr = window.devicePixelRatio || 1;
      const w = Math.round(canvas.clientWidth * dpr);
      const h = Math.round(canvas.clientHeight * dpr);
      if (canvas.width !== w) canvas.width = w;
      if (canvas.height !== h) canvas.height = h;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      ctx.clearRect(0, 0, w, h);
      if (!current?.length || !w || !h) return;
      // More buckets than pixels: the loudest per pixel; fewer: each pixel takes its bucket's.
      const fit = current.length >= w ? bucketPeaks([current], w) : null;
      ctx.fillStyle = getComputedStyle(canvas).color;
      const mid = h / 2;
      for (let x = 0; x < w; x++) {
        const v = fit ? fit[x] : current[Math.floor((x * current.length) / w)];
        const half = Math.max(dpr / 2, Math.min(1, v) * mid);
        ctx.fillRect(x, mid - half, 1, half * 2);
      }
    }
    const resized = new ResizeObserver(draw);
    resized.observe(canvas);
    draw();
    return {
      update(next: Float32Array | null) {
        if (next === current) return;
        current = next;
        draw();
      },
      destroy() {
        resized.disconnect();
      },
    };
  }
</script>

<div class="lanes" class:locked style:grid-template-rows={rows}>
  <div class="label ruler-label"></div>
  <div class="ruler" aria-hidden="true">
    <div class="area">
      {#each ticks as t (t.ms)}
        <span style:left="{t.pct}%">{t.label}</span>
      {/each}
    </div>
  </div>

  <div class="label video-label">
    <span class="name">Video</span>
    <button
      class="btn btn-sm blur"
      class:on={blur}
      aria-pressed={blur}
      disabled={locked}
      title="For clips where only the sound matters: uploads and exports blur the picture"
      onclick={() => onblur(!blur)}
    >
      <Icon name="eye" size={14} /> Blur
    </button>
  </div>
  <div class="lane video">{@render video()}</div>

  <div class="label zoom-label">
    <span class="name">Zoom</span>
  </div>
  <div class="lane zoom">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="area"
      onpointerdown={(e) => {
        onlanedown?.();
        onzoomselect(null);
        grab(e);
      }}
      onpointermove={move}
      onpointerup={release}
      onpointercancel={release}
      onlostpointercapture={release}
    >
      {#each shading.dims as d, i (i)}
        <div class="dim" style:left="{d.left}%" style:width="{d.width}%"></div>
      {/each}
      {#if zooms.length === 0}
        <span class="zoom-hint">Press <kbd>Z</kbd> to zoom in at the playhead</span>
      {/if}
      {#each zooms as z, i (i)}
        <!-- A pointer target for placing the zoom in time; Enter picks it from the keyboard. -->
        <div
          class="zoom-block"
          class:selected={selectedZoom === i}
          class:moving={zoomDrag?.i === i && zoomDrag.what === "move"}
          class:resizing={zoomDrag?.i === i && zoomDrag.what !== "move"}
          role="button"
          tabindex="0"
          aria-label="Zoom {z.zoom.toFixed(1)}× from {(z.startMs / 1000).toFixed(1)} s"
          aria-pressed={selectedZoom === i}
          title="Drag to move, drag an edge to make it longer or shorter, Delete to remove"
          style:left="{msToX(z.startMs, duration, 100)}%"
          style:width="{msToX(z.endMs, duration, 100) - msToX(z.startMs, duration, 100)}%"
          onpointerdown={(e) => zoomDown(e, i)}
          onpointermove={zoomMove}
          onpointerup={zoomUp}
          onpointercancel={zoomUp}
          onlostpointercapture={zoomUp}
          onkeydown={(e) => zoomKey(e, i)}
        >
          <!-- Where a press resizes instead of moving (as wide as EDGE_PX): shows the resize cursor. -->
          <span class="zoom-edge start" aria-hidden="true"></span>
          <span class="zoom-edge end" aria-hidden="true"></span>
          <span class="ramp in" style:width="{(rampMs(z) / (z.endMs - z.startMs)) * 100}%"></span>
          <span class="zoom-text"><Icon name="zoom" size={12} /> {z.zoom.toFixed(1)}×</span>
          <span class="ramp out" style:width="{(rampMs(z) / (z.endMs - z.startMs)) * 100}%"></span>
          {#if selectedZoom === i && !locked}
            <button
              class="zoom-remove"
              aria-label="Remove this zoom"
              title="Remove this zoom (Delete)"
              onpointerdown={(e) => e.stopPropagation()}
              onclick={(e) => {
                e.stopPropagation();
                onzoomremove(i);
              }}
            >
              <Icon name="close" size={11} />
            </button>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  <div class="label zoom-label">
    <span class="name">Subtitles</span>
  </div>
  <div class="lane zoom">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="area"
      onpointerdown={(e) => {
        onlanedown?.();
        onsubtitleselect(null);
        grab(e);
      }}
      onpointermove={move}
      onpointerup={release}
      onpointercancel={release}
      onlostpointercapture={release}
    >
      {#each shading.dims as d, i (i)}
        <div class="dim" style:left="{d.left}%" style:width="{d.width}%"></div>
      {/each}
      {#if subtitles.length === 0}
        <span class="zoom-hint">Press <kbd>T</kbd> to add a subtitle at the playhead</span>
      {/if}
      {#each subtitles as s, i (i)}
        <!-- A pointer target for placing the subtitle in time; Enter edits its text. -->
        <div
          class="zoom-block sub-block"
          class:selected={selectedSubtitle === i}
          class:moving={subDrag?.i === i && subDrag.what === "move"}
          class:resizing={subDrag?.i === i && subDrag.what !== "move"}
          role="button"
          tabindex="0"
          aria-label="Subtitle “{s.text}” from {(s.startMs / 1000).toFixed(1)} s"
          aria-pressed={selectedSubtitle === i}
          title="Double-click to edit the text, drag to move, drag an edge to make it longer or shorter, Delete to remove"
          style:left="{msToX(s.startMs, duration, 100)}%"
          style:width="{msToX(s.endMs, duration, 100) - msToX(s.startMs, duration, 100)}%"
          onpointerdown={(e) => subDown(e, i)}
          onpointermove={subMove}
          onpointerup={subUp}
          onpointercancel={subUp}
          onlostpointercapture={subUp}
          ondblclick={() => {
            if (!locked) onsubtitleedit(i);
          }}
          onkeydown={(e) => subKey(e, i)}
        >
          <span class="zoom-edge start" aria-hidden="true"></span>
          <span class="zoom-edge end" aria-hidden="true"></span>
          <span class="zoom-text sub-text">{s.text || "…"}</span>
          {#if selectedSubtitle === i && !locked}
            <button
              class="zoom-remove"
              aria-label="Remove this subtitle"
              title="Remove this subtitle (Delete)"
              onpointerdown={(e) => e.stopPropagation()}
              onclick={(e) => {
                e.stopPropagation();
                onsubtitleremove(i);
              }}
            >
              <Icon name="close" size={11} />
            </button>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  {#each tracks as t, i (t.key)}
    <div class="label" class:last={i === tracks.length - 1}>
      <div class="line">
        <span class="name" title={t.name}>{t.name}</span>
        <output>{level(t.volume)}</output>
      </div>
      <div class="line">
        <button
          class="mute"
          class:on={t.muted}
          aria-pressed={t.muted}
          aria-label="Mute {t.name}"
          title={t.muted ? `Unmute ${t.name}` : `Mute ${t.name}`}
          disabled={locked}
          onclick={() => onmute(t.key)}
        >
          M
        </button>
        <input
          class="range"
          type="range"
          min="0"
          max="100"
          step="1"
          value={t.volume}
          disabled={locked}
          style:--fill="{t.volume}%"
          aria-label="{t.name} volume"
          oninput={(e) => onvolume(t.key, Number(e.currentTarget.value))}
        />
      </div>
    </div>
    <div class="lane audio" class:last={i === tracks.length - 1}>
      <!-- A pointer target for seeking, like the video lane's track; keyboard users seek with the
           player controls and the arrow keys. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="area"
        onpointerdown={grab}
        onpointermove={move}
        onpointerup={release}
        onpointercancel={release}
        onlostpointercapture={release}
      >
        <canvas class="wave {t.key.startsWith('track-') ? 'track' : t.key}" class:muted={t.muted} use:waveform={t.peaks}></canvas>
        {#each shading.dims as d, i (i)}
          <div class="dim" style:left="{d.left}%" style:width="{d.width}%"></div>
        {/each}
        {#each shading.cuts as c (c.left)}
          <div class="cut" style:left="{c.left}%" style:width="{c.width}%"></div>
        {/each}
      </div>
    </div>
  {/each}


  {#if tracks.length}
    <div class="playhead-span" style:grid-row="3 / span {tracks.length + 2}" aria-hidden="true">
      <div class="playhead" style:left="{msToX(position, duration, 100)}%"></div>
    </div>
  {/if}
</div>

<style>
  /* The lanes' time area is inset as far as TrimBar's track (its trim handles sit in the gap), so
     every lane's positions line up with the video lane's. */
  .lanes {
    --inset: 0.9rem;
    position: relative;
    display: grid;
    grid-template-columns: 170px minmax(0, 1fr);
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    user-select: none;
  }

  .label {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.3rem;
    min-width: 0;
    padding: 0.4rem 0.75rem;
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }

  .video-label {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
  }

  .line {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    flex: 1;
    font-size: 0.85rem;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  output {
    color: var(--text-faint);
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
  }

  .range {
    flex: 1;
    min-width: 0;
  }

  .mute,
  .mute-space {
    flex: none;
    width: 1.35rem;
    height: 1.35rem;
  }

  .mute {
    display: grid;
    place-items: center;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text-muted);
    font: inherit;
    font-size: 0.7rem;
    font-weight: 700;
    cursor: pointer;
    transition:
      color 0.15s,
      border-color 0.15s,
      background 0.15s;
  }

  .mute:hover:not(:disabled) {
    color: var(--text);
  }

  .mute.on {
    border-color: var(--warning);
    background: var(--warning-soft);
    color: var(--warning);
  }

  .blur {
    height: 1.7rem;
    padding: 0 0.55rem;
    font-size: 0.8rem;
  }

  .blur.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-text);
  }

  /* While the clip uploads, the edit can't change. */
  .locked .mute,
  .locked .range,
  .locked .blur {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .ruler-label,
  .ruler {
    height: 1.4rem;
    padding: 0;
  }

  .ruler {
    position: relative;
    border-bottom: 1px solid var(--border);
    color: var(--text-faint);
    font-size: 0.68rem;
    font-variant-numeric: tabular-nums;
  }

  .ruler span {
    position: absolute;
    top: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    padding-left: 0.25rem;
    border-left: 1px solid var(--border-strong);
    white-space: nowrap;
  }

  .lane {
    position: relative;
    min-width: 0;
    border-bottom: 1px solid var(--border);
  }

  /* The trim bar's split buttons hang below it, over the first audio lane. */
  .lane.video {
    z-index: 2;
  }

  .lane.audio {
    height: 3.4rem;
    background: var(--bg);
  }

  .lane.zoom {
    height: 2.3rem;
    background: var(--bg);
  }

  .lane.zoom .area {
    cursor: pointer;
    touch-action: none;
  }

  .zoom-hint {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-faint);
    font-size: 0.75rem;
    pointer-events: none;
  }

  .zoom-hint kbd {
    margin: 0 0.3rem;
    padding: 0 0.35rem;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--text-muted);
    font: inherit;
  }

  .zoom-block {
    position: absolute;
    top: 0.35rem;
    bottom: 0.35rem;
    display: flex;
    align-items: center;
    overflow: hidden;
    box-sizing: border-box;
    min-width: 6px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--accent-text);
    cursor: grab;
    touch-action: none;
    /* The edges resize. */
    box-shadow:
      inset 3px 0 0 var(--accent),
      inset -3px 0 0 var(--accent);
  }

  /* While dragging, the block holds the pointer, so its own cursor is the one that shows. */
  .zoom-block.moving {
    cursor: grabbing;
  }

  .zoom-block.resizing {
    cursor: ew-resize;
  }

  .zoom-edge {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 1;
    width: 10px;
    cursor: ew-resize;
  }
  .zoom-edge.start {
    left: 0;
  }
  .zoom-edge.end {
    right: 0;
  }

  .locked .zoom-edge {
    cursor: not-allowed;
  }

  .zoom-block.selected {
    border-color: #fff;
    box-shadow:
      inset 3px 0 0 #fff,
      inset -3px 0 0 #fff,
      0 0 0 1px var(--accent);
  }

  .zoom-block:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  /* The ease in and out: lighter where the zoom is still on its way. */
  .ramp {
    flex: none;
    align-self: stretch;
    pointer-events: none;
  }
  .ramp.in {
    background: linear-gradient(to right, transparent, var(--accent-soft));
  }
  .ramp.out {
    background: linear-gradient(to left, transparent, var(--accent-soft));
  }

  .zoom-text {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 0.2rem;
    min-width: 0;
    overflow: hidden;
    padding-left: 0.3rem;
    font-size: 0.72rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
  }

  .sub-text {
    display: block;
    padding-right: 0.3rem;
    font-weight: 600;
    text-overflow: ellipsis;
  }

  .zoom-remove {
    display: grid;
    flex: none;
    place-items: center;
    width: 1.1rem;
    height: 1.1rem;
    /* Clear of the end edge, which resizes (EDGE_PX wide): a press there must never remove. */
    margin-right: calc(10px + 0.3rem);
    padding: 0;
    border: none;
    border-radius: 4px;
    background: rgba(5, 4, 10, 0.55);
    color: #fff;
    cursor: pointer;
  }

  .locked .zoom-block {
    cursor: not-allowed;
  }

  /* The lanes' rounded border closes the last row. */
  .lane.last,
  .label.last {
    border-bottom: none;
  }

  .area {
    position: absolute;
    inset: 0 var(--inset);
  }

  .ruler .area {
    overflow: hidden;
  }

  .lane.audio .area {
    cursor: pointer;
    touch-action: none;
  }

  .wave {
    position: absolute;
    top: 0.3rem;
    left: 0;
    width: 100%;
    height: calc(100% - 0.6rem);
    color: var(--accent);
    opacity: 0.8;
    pointer-events: none;
  }

  .wave.mic {
    color: var(--success);
  }

  .wave.voice {
    color: var(--warning);
  }

  .wave.muted {
    opacity: 0.3;
  }

  /* As on the video lane: dimmed outside the trim, red hatching over removed parts. */
  .dim {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgba(5, 4, 10, 0.6);
    pointer-events: none;
  }

  .cut {
    position: absolute;
    top: 0;
    bottom: 0;
    background: repeating-linear-gradient(135deg, rgba(255, 93, 108, 0.32) 0 6px, rgba(255, 93, 108, 0.1) 6px 12px);
    box-shadow:
      inset 2px 0 0 var(--danger),
      inset -2px 0 0 var(--danger);
    pointer-events: none;
  }

  /* The playhead carries on from the video lane's down through the audio lanes. */
  .playhead-span {
    position: absolute;
    grid-column: 2;
    top: 0;
    bottom: 0;
    left: var(--inset);
    right: var(--inset);
    z-index: 3;
    pointer-events: none;
  }

  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: #fff;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.35);
  }
</style>
