<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Piece } from "../lib/cuts";
  import { blockLayout, laneShading, rulerTicks } from "../lib/lanes";
  import type { TrimRange } from "../lib/trim";
  import {
    fullView,
    inView,
    isZoomed,
    maxScale,
    panView,
    scaleToSlider,
    scrollThumb,
    sliderToScale,
    viewAt,
    viewMs,
    viewPct,
    viewScale,
    zoomView,
    type TimeView,
  } from "../lib/timelineView";
  import { windowPeaks } from "../lib/waveform";
  import { edgeScroller, edgeSpeed } from "../lib/edgeScroll";
  import type { AudioRange, BlurRange, SubtitleRange, ZoomRange } from "../lib/api";
  import { addPart, levelLabel, movePart, MIN_AUDIO_PART_MS, partsOn, resizePart, spanPart } from "../lib/audioParts";
  import { moveZoom, rampMs, resizeZoom } from "../lib/zoom";
  import { moveSubtitle, resizeSubtitle } from "../lib/subtitles";
  import { moveBlur, resizeBlur } from "../lib/blur";
  import Icon from "./Icon.svelte";

  interface LaneTrack {
    /** The audio engine's key: "game", "mic", "voice" or "track-<index>". */
    key: string;
    /** The track's index in the file (0 = the game), as muted parts name it. */
    track: number;
    name: string;
    /** The track's waveform (0–1 per bucket); null while it isn't loaded. */
    peaks: Float32Array | null;
    /** The share of the clip (0–1) `peaks` covers: all of it, or a part around a zoomed-in view. */
    peaksFrom: number;
    peaksTo: number;
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
    blurs,
    selectedBlur,
    onblurselect,
    onblurchange,
    onblurgesture,
    onblurremove,
    subtitles,
    selectedSubtitle,
    onsubtitleselect,
    onsubtitlechange,
    onsubtitlegesture,
    onsubtitleremove,
    onsubtitleedit,
    onlanedown,
    audioParts,
    selectedPart,
    onpartselect,
    onpartchange,
    onpartgesture,
    onpartremove,
    onpartlevel,
    onpartadd,
    view,
    onview,
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
    /** The blurred parts, sorted, in ms into the file. */
    blurs: BlurRange[];
    /** The picked blurred part's index, or null. */
    selectedBlur: number | null;
    onblurselect: (i: number | null) => void;
    onblurchange: (blurs: BlurRange[]) => void;
    /** A drag of a blurred part started (true) or ended (false): one drag is one undo step. */
    onblurgesture: (active: boolean) => void;
    onblurremove: (i: number) => void;
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
    /** The muted and turned-down parts of every track, sorted by track, then time. */
    audioParts: AudioRange[];
    /** The picked part's index in `audioParts`, or null. */
    selectedPart: number | null;
    onpartselect: (i: number | null) => void;
    onpartchange: (parts: AudioRange[]) => void;
    /** A drag of a part (or drawing one) started (true) or ended (false): one drag is one undo step. */
    onpartgesture: (active: boolean) => void;
    onpartremove: (i: number) => void;
    /** The level button of part `i`: Muted → 25% → 50%. */
    onpartlevel: (i: number) => void;
    /** "Mute a part" on a lane: adds one on `track` at the playhead. */
    onpartadd: (track: number) => void;
    /** The stretch of the clip the lanes show (the timeline's zoom). */
    view: TimeView;
    /** The user zoomed or scrolled the timeline. */
    onview: (view: TimeView) => void;
    /** The video lane: the TrimBar. */
    video: Snippet;
  } = $props();

  const ticks = $derived(rulerTicks(duration, view));
  const shading = $derived(laneShading(duration, trim, pieces, view));
  /** Zoomed in: the lanes clip what runs off screen and a scrollbar shows under them. */
  const zoomed = $derived(isZoomed(view, duration));
  /** Where `ms` is on the lanes, in percent (outside 0–100 when off screen). */
  const pct = (ms: number) => viewPct(ms, view);
  /** The Zoom, Blur and Subtitles rows show only while they have something on them. */
  const extraRows = $derived((zooms.length > 0 ? 1 : 0) + (blurs.length > 0 ? 1 : 0) + (subtitles.length > 0 ? 1 : 0));
  /**
   * Ruler, video, zoom and subtitles (when used), one row per track, and the scrollbar while
   * zoomed in: the playhead spans the rows from zoom to the last track.
   */
  const rows = $derived(
    ["auto", "auto", ...Array(extraRows).fill("auto"), ...tracks.map(() => "auto"), ...(zoomed ? ["auto"] : [])].join(" "),
  );

  const level = (v: number) => (v === 0 ? "Off" : `${v}%`);

  /** The lane being dragged on (its pointer is captured). */
  let scrubbing: EventTarget | null = null;

  function msAt(e: PointerEvent, area: HTMLElement): number {
    const rect = area.getBoundingClientRect();
    return viewMs(e.clientX - rect.left, rect.width, view);
  }

  /**
   * The drag in progress (a scrub, or a zoom or subtitle block): its lane's time area, its pointer
   * as last seen, and what a pointer at a new place does. Held at an edge of a zoomed-in timeline,
   * the view moves along and the drag goes on with it.
   */
  let edgeDrag: { area: HTMLElement; e: PointerEvent; apply: (e: PointerEvent) => void } | null = null;
  const autoScroll = edgeScroller((dt) => {
    if (!edgeDrag) return;
    const rect = edgeDrag.area.getBoundingClientRect();
    const speed = edgeSpeed(edgeDrag.e.clientX - rect.left, rect.width);
    if (!speed) return;
    const next = panView(view, duration, (speed * dt * (view.to - view.from)) / 1000);
    if (next === view) return;
    onview(next);
    edgeDrag.apply(edgeDrag.e);
  });
  $effect(() => () => autoScroll.stop());

  function startDrag(area: HTMLElement, e: PointerEvent, apply: (e: PointerEvent) => void) {
    edgeDrag = { area, e, apply };
    autoScroll.start();
  }

  function endDrag() {
    edgeDrag = null;
    autoScroll.stop();
  }

  // Like the video lane's track: the pointer seeks, and a drag scrubs.
  function grab(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const area = e.currentTarget as HTMLElement;
    area.setPointerCapture(e.pointerId);
    scrubbing = area;
    startDrag(area, e, (ev) => onseek(msAt(ev, area)));
    onscrubstart?.();
    onseek(msAt(e, area));
  }

  function move(e: PointerEvent) {
    if (scrubbing === null || scrubbing !== e.currentTarget) return;
    if (edgeDrag) edgeDrag.e = e;
    onseek(msAt(e, e.currentTarget as HTMLElement));
  }

  function release() {
    if (scrubbing === null) return;
    scrubbing = null;
    endDrag();
    onscrubend?.();
  }

  /** A zoom being dragged on the Zoom lane: its body (moves it) or one edge (resizes it). */
  let zoomDrag: { i: number; what: "move" | "start" | "end"; offset: number; moved: boolean } | null = $state(null);
  /** The lanes' width in px (the ruler's), to lay out short blocks. */
  let laneWidth = $state(0);
  /** How a block from `startMs` to `endMs` is laid out on the lane. */
  const layoutOf = (startMs: number, endMs: number) =>
    blockLayout(((endMs - startMs) / Math.max(1, view.to - view.from)) * laneWidth);
  /** Whether a press at `x` on `block` grabs an edge (resizes) or the middle (moves). */
  function grabbed(block: HTMLElement, x: number): "move" | "start" | "end" {
    const r = block.getBoundingClientRect();
    const edge = blockLayout(r.width).edgePx;
    return x - r.left <= edge ? "start" : r.right - x <= edge ? "end" : "move";
  }

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
    const what = grabbed(block, e.clientX);
    block.setPointerCapture(e.pointerId);
    zoomDrag = { i, what, offset: msAt(e, area) - z.startMs, moved: false };
    startDrag(area, e, (ev) => zoomTo(ev, area));
    onzoomgesture(true);
  }

  function zoomMove(e: PointerEvent) {
    if (!zoomDrag) return;
    if (edgeDrag) edgeDrag.e = e;
    zoomTo(e, (e.currentTarget as HTMLElement).parentElement as HTMLElement);
  }

  /** The dragged zoom follows a pointer at `e` on `area`. */
  function zoomTo(e: PointerEvent, area: HTMLElement) {
    if (!zoomDrag) return;
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
    endDrag();
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

  /** A blurred part being dragged on the Blur lane: its body (moves it) or one edge (resizes it). */
  let blurDrag: { i: number; what: "move" | "start" | "end"; offset: number } | null = $state(null);

  function blurDown(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    onlanedown?.();
    e.preventDefault();
    e.stopPropagation();
    const block = e.currentTarget as HTMLElement;
    const area = block.parentElement as HTMLElement;
    const b = blurs[i];
    onblurselect(i);
    onseek(b.startMs);
    if (locked) return;
    const what = grabbed(block, e.clientX);
    block.setPointerCapture(e.pointerId);
    blurDrag = { i, what, offset: msAt(e, area) - b.startMs };
    startDrag(area, e, (ev) => blurTo(ev, area));
    onblurgesture(true);
  }

  function blurMove(e: PointerEvent) {
    if (!blurDrag) return;
    if (edgeDrag) edgeDrag.e = e;
    blurTo(e, (e.currentTarget as HTMLElement).parentElement as HTMLElement);
  }

  /** The dragged blurred part follows a pointer at `e` on `area`. */
  function blurTo(e: PointerEvent, area: HTMLElement) {
    if (!blurDrag) return;
    const ms = msAt(e, area);
    const { i, what } = blurDrag;
    const next = what === "move" ? moveBlur(blurs, i, ms - blurDrag.offset, trim) : resizeBlur(blurs, i, what, ms, trim);
    if (next !== blurs) onblurchange(next);
  }

  function blurUp() {
    if (!blurDrag) return;
    const { i, what } = blurDrag;
    blurDrag = null;
    endDrag();
    onblurgesture(false);
    const b = blurs[i];
    if (b) onseek(what === "end" ? Math.max(b.startMs, b.endMs - 1) : b.startMs);
  }

  function blurKey(e: KeyboardEvent, i: number) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      onblurselect(i);
      onseek(blurs[i].startMs);
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
    const what = grabbed(block, e.clientX);
    block.setPointerCapture(e.pointerId);
    subDrag = { i, what, offset: msAt(e, area) - s.startMs };
    startDrag(area, e, (ev) => subTo(ev, area));
    onsubtitlegesture(true);
  }

  function subMove(e: PointerEvent) {
    if (!subDrag) return;
    if (edgeDrag) edgeDrag.e = e;
    subTo(e, (e.currentTarget as HTMLElement).parentElement as HTMLElement);
  }

  /** The dragged subtitle follows a pointer at `e` on `area`. */
  function subTo(e: PointerEvent, area: HTMLElement) {
    if (!subDrag) return;
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
    endDrag();
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

  /**
   * A part being dragged on an audio lane: its body (moves it), one edge (resizes it), or a new one
   * being drawn with Shift held (stretches from where the drag started).
   */
  let partDrag: { i: number; what: "move" | "start" | "end" | "span"; offset: number; anchor: number } | null =
    $state(null);

  /** A press on an audio lane: Shift draws a new muted part; otherwise it seeks and scrubs. */
  function laneDown(e: PointerEvent, track: number) {
    onlanedown?.();
    if (e.button === 0 && e.shiftKey && !locked) {
      const area = e.currentTarget as HTMLElement;
      const ms = msAt(e, area);
      const added = addPart(audioParts, track, ms, trim, MIN_AUDIO_PART_MS);
      if (added) {
        e.preventDefault();
        area.setPointerCapture(e.pointerId);
        onpartgesture(true);
        onpartchange(added.parts);
        onpartselect(added.index);
        partDrag = { i: added.index, what: "span", offset: 0, anchor: added.parts[added.index].startMs };
        startDrag(area, e, (ev) => partTo(ev, area));
        return;
      }
    }
    onpartselect(null);
    grab(e);
  }

  function laneMove(e: PointerEvent) {
    if (partDrag?.what === "span") {
      if (edgeDrag) edgeDrag.e = e;
      partTo(e, e.currentTarget as HTMLElement);
      return;
    }
    move(e);
  }

  function laneUp() {
    if (partDrag?.what === "span") partUp();
    else release();
  }

  function partDown(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    onlanedown?.();
    e.preventDefault();
    e.stopPropagation();
    const block = e.currentTarget as HTMLElement;
    const area = block.parentElement as HTMLElement;
    const p = audioParts[i];
    if (!p) return;
    onpartselect(i);
    onseek(p.startMs);
    if (locked) return;
    const what = grabbed(block, e.clientX);
    block.setPointerCapture(e.pointerId);
    partDrag = { i, what, offset: msAt(e, area) - p.startMs, anchor: p.startMs };
    startDrag(area, e, (ev) => partTo(ev, area));
    onpartgesture(true);
  }

  function partMove(e: PointerEvent) {
    if (!partDrag) return;
    if (edgeDrag) edgeDrag.e = e;
    partTo(e, (e.currentTarget as HTMLElement).parentElement as HTMLElement);
  }

  /** The dragged part follows a pointer at `e` on `area`. */
  function partTo(e: PointerEvent, area: HTMLElement) {
    if (!partDrag) return;
    const ms = msAt(e, area);
    const { i, what } = partDrag;
    const next =
      what === "span"
        ? spanPart(audioParts, i, partDrag.anchor, ms, trim)
        : what === "move"
          ? movePart(audioParts, i, ms - partDrag.offset, trim)
          : resizePart(audioParts, i, what, ms, trim);
    if (next !== audioParts) onpartchange(next);
  }

  function partUp() {
    if (!partDrag) return;
    const { i, what } = partDrag;
    partDrag = null;
    endDrag();
    onpartgesture(false);
    const p = audioParts[i];
    if (p) onseek(what === "end" ? Math.max(p.startMs, p.endMs - 1) : p.startMs);
  }

  function partKey(e: KeyboardEvent, i: number) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      onpartselect(i);
      onseek(audioParts[i].startMs);
    }
  }

  /** The share of a track's peaks (0–1) the view shows, for peaks covering `peaksFrom`–`peaksTo` of the clip. */
  function waveShare(t: LaneTrack): { from: number; to: number } {
    const d = Math.max(1, duration);
    const cover = Math.max(1e-9, t.peaksTo - t.peaksFrom);
    return { from: (view.from / d - t.peaksFrom) / cover, to: (view.to / d - t.peaksFrom) / cover };
  }

  /** The ruler's time area: where the pointer is in time, for zooming around it. */
  let rulerArea = $state<HTMLElement>();

  /** The point to zoom around from the slider or buttons: the playhead while on screen, else the middle. */
  const zoomAnchor = () => (inView(position, view) ? position : (view.from + view.to) / 2);
  const canZoom = $derived(maxScale(duration) > 1);
  const scale = $derived(viewScale(view, duration));

  function zoomScale(next: number) {
    onview(zoomView(view, duration, next, zoomAnchor()));
  }

  /**
   * Ctrl+scroll (or a pinch) zooms around the pointer; while zoomed in, a sideways scroll (or
   * Shift+scroll) moves along the clip. A plain scroll is left to the page. Not passive, so the
   * page doesn't scroll (or zoom) as well.
   */
  function wheel(node: HTMLElement) {
    function onwheel(e: WheelEvent) {
      if (!rulerArea || !(duration > 0)) return;
      const rect = rulerArea.getBoundingClientRect();
      const unit = e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? rect.width : 1;
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        if (!canZoom) return;
        const anchor = viewMs(e.clientX - rect.left, rect.width, view);
        onview(zoomView(view, duration, scale * Math.exp(-e.deltaY * unit * 0.0025), anchor));
        return;
      }
      if (!zoomed) return;
      const sideways = e.shiftKey ? e.deltaX || e.deltaY : Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : 0;
      if (!sideways) return;
      e.preventDefault();
      onview(panView(view, duration, ((sideways * unit) / rect.width) * (view.to - view.from)));
    }
    node.addEventListener("wheel", onwheel, { passive: false });
    return { destroy: () => node.removeEventListener("wheel", onwheel) };
  }

  /** The scrollbar: dragging its thumb (or pressing beside it) moves along the clip. */
  let scrollTrack = $state<HTMLElement>();
  let scrollDrag: { x: number; from: number } | null = $state(null);
  const thumbBox = $derived(scrollThumb(view, duration));

  function scrollDown(e: PointerEvent) {
    if (e.button !== 0 || !scrollTrack) return;
    e.preventDefault();
    const rect = scrollTrack.getBoundingClientRect();
    let from = view.from;
    // Beside the thumb: centre the view there first, then drag on from it.
    if (!(e.target as HTMLElement).closest(".scroll-thumb")) {
      const at = ((e.clientX - rect.left) / rect.width) * duration;
      const next = viewAt(view, duration, at - (view.to - view.from) / 2);
      onview(next);
      from = next.from;
    }
    scrollTrack.setPointerCapture(e.pointerId);
    scrollDrag = { x: e.clientX, from };
  }

  function scrollMove(e: PointerEvent) {
    if (!scrollDrag || !scrollTrack) return;
    const width = scrollTrack.getBoundingClientRect().width;
    if (width > 0) onview(viewAt(view, duration, scrollDrag.from + ((e.clientX - scrollDrag.x) / width) * duration));
  }

  function scrollUp() {
    scrollDrag = null;
  }

  /**
   * Draws the part of `peaks` the view shows across the canvas in device pixels, again whenever
   * the canvas is resized or the view moves.
   */
  function waveform(canvas: HTMLCanvasElement, initial: { peaks: Float32Array | null; from: number; to: number }) {
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
      if (!current.peaks?.length || !w || !h) return;
      // More buckets than pixels: the loudest per pixel; fewer: each pixel takes its bucket's.
      const fit = windowPeaks(current.peaks, current.from, current.to, w);
      ctx.fillStyle = getComputedStyle(canvas).color;
      const mid = h / 2;
      for (let x = 0; x < w; x++) {
        const v = fit[x];
        const half = Math.max(dpr / 2, Math.min(1, v) * mid);
        ctx.fillRect(x, mid - half, 1, half * 2);
      }
    }
    const resized = new ResizeObserver(draw);
    resized.observe(canvas);
    draw();
    return {
      update(next: { peaks: Float32Array | null; from: number; to: number }) {
        if (next.peaks === current.peaks && next.from === current.from && next.to === current.to) return;
        current = next;
        draw();
      },
      destroy() {
        resized.disconnect();
      },
    };
  }
</script>

<div class="lanes" class:locked class:zoomed style:grid-template-rows={rows} use:wheel>
  <div class="label ruler-label">
    <div class="timeline-zoom" title="Zoom the timeline (Ctrl+scroll, + and −)">
      <button
        class="zoom-step"
        aria-label="Zoom the timeline out"
        disabled={!canZoom || scale <= 1}
        onclick={() => zoomScale(scale / 2)}
      >
        <Icon name="zoomOut" size={13} />
      </button>
      <input
        class="range"
        type="range"
        min="0"
        max="1000"
        step="1"
        value={Math.round(scaleToSlider(scale, duration) * 1000)}
        disabled={!canZoom}
        style:--fill="{scaleToSlider(scale, duration) * 100}%"
        aria-label="Timeline zoom"
        aria-valuetext="{scale < 10 ? scale.toFixed(1) : Math.round(scale)}×"
        oninput={(e) => zoomScale(sliderToScale(Number(e.currentTarget.value) / 1000, duration))}
      />
      <button
        class="zoom-step"
        aria-label="Zoom the timeline in"
        disabled={!canZoom || scale >= maxScale(duration)}
        onclick={() => zoomScale(scale * 2)}
      >
        <Icon name="zoom" size={13} />
      </button>
    </div>
  </div>
  <div class="ruler" aria-hidden="true">
    <div class="area" bind:this={rulerArea} bind:clientWidth={laneWidth}>
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
      title="For clips where only the sound matters: uploads and exports blur the whole picture (to blur just a part, use Blur under the player or B)"
      onclick={() => onblur(!blur)}
    >
      <Icon name="eye" size={14} /> Blur
    </button>
  </div>
  <div class="lane video">{@render video()}</div>

  {#if zooms.length > 0}
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
      {#each zooms as z, i (i)}
        {@const fit = layoutOf(z.startMs, z.endMs)}
        <!-- A pointer target for placing the zoom in time; Enter picks it from the keyboard. -->
        <div
          class="zoom-block"
          class:compact={!fit.showText}
          class:selected={selectedZoom === i}
          class:moving={zoomDrag?.i === i && zoomDrag.what === "move"}
          class:resizing={zoomDrag?.i === i && zoomDrag.what !== "move"}
          role="button"
          tabindex="0"
          aria-label="Zoom {z.zoom.toFixed(1)}× from {(z.startMs / 1000).toFixed(1)} s"
          aria-pressed={selectedZoom === i}
          title="Drag to move, drag an edge to make it longer or shorter, Delete to remove"
          style:left="{pct(z.startMs)}%"
          style:width="{pct(z.endMs) - pct(z.startMs)}%"
          onpointerdown={(e) => zoomDown(e, i)}
          onpointermove={zoomMove}
          onpointerup={zoomUp}
          onpointercancel={zoomUp}
          onlostpointercapture={zoomUp}
          onkeydown={(e) => zoomKey(e, i)}
        >
          <!-- Where a press resizes instead of moving (`blockLayout`'s edge): shows the resize cursor. -->
          <span class="zoom-edge start" aria-hidden="true" style:width="{fit.edgePx}px"></span>
          <span class="zoom-edge end" aria-hidden="true" style:width="{fit.edgePx}px"></span>
          <span class="ramp in" style:width="{(rampMs(z) / (z.endMs - z.startMs)) * 100}%"></span>
          {#if fit.showText}
            <span class="zoom-text"><Icon name="zoom" size={12} /> {z.zoom.toFixed(1)}×</span>
          {/if}
          <span class="ramp out" style:width="{(rampMs(z) / (z.endMs - z.startMs)) * 100}%"></span>
          {#if selectedZoom === i && !locked && fit.showRemove}
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

  {/if}

  {#if blurs.length > 0}
  <div class="label zoom-label">
    <span class="name">Blur</span>
  </div>
  <div class="lane zoom">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="area"
      onpointerdown={(e) => {
        onlanedown?.();
        onblurselect(null);
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
      {#each blurs as b, i (i)}
        {@const fit = layoutOf(b.startMs, b.endMs)}
        <!-- A pointer target for placing the blurred part in time; Enter picks it from the keyboard. -->
        <div
          class="zoom-block blur-block"
          class:compact={!fit.showText}
          class:selected={selectedBlur === i}
          class:moving={blurDrag?.i === i && blurDrag.what === "move"}
          class:resizing={blurDrag?.i === i && blurDrag.what !== "move"}
          role="button"
          tabindex="0"
          aria-label="Blurred from {(b.startMs / 1000).toFixed(1)} s to {(b.endMs / 1000).toFixed(1)} s"
          aria-pressed={selectedBlur === i}
          title="Drag to move, drag an edge to make it longer or shorter, Delete to remove"
          style:left="{pct(b.startMs)}%"
          style:width="{pct(b.endMs) - pct(b.startMs)}%"
          onpointerdown={(e) => blurDown(e, i)}
          onpointermove={blurMove}
          onpointerup={blurUp}
          onpointercancel={blurUp}
          onlostpointercapture={blurUp}
          onkeydown={(e) => blurKey(e, i)}
        >
          <span class="zoom-edge start" aria-hidden="true" style:width="{fit.edgePx}px"></span>
          <span class="zoom-edge end" aria-hidden="true" style:width="{fit.edgePx}px"></span>
          {#if fit.showText}
            <span class="zoom-text"><Icon name="eye" size={12} /> Blur</span>
          {/if}
          {#if selectedBlur === i && !locked && fit.showRemove}
            <button
              class="zoom-remove"
              aria-label="Remove this blurred part"
              title="Remove this blurred part (Delete)"
              onpointerdown={(e) => e.stopPropagation()}
              onclick={(e) => {
                e.stopPropagation();
                onblurremove(i);
              }}
            >
              <Icon name="close" size={11} />
            </button>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  {/if}

  {#if subtitles.length > 0}
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
      {#each subtitles as s, i (i)}
        {@const fit = layoutOf(s.startMs, s.endMs)}
        <!-- A pointer target for placing the subtitle in time; Enter edits its text. -->
        <div
          class="zoom-block sub-block"
          class:compact={!fit.showText}
          class:selected={selectedSubtitle === i}
          class:moving={subDrag?.i === i && subDrag.what === "move"}
          class:resizing={subDrag?.i === i && subDrag.what !== "move"}
          role="button"
          tabindex="0"
          aria-label="Subtitle “{s.text}” from {(s.startMs / 1000).toFixed(1)} s"
          aria-pressed={selectedSubtitle === i}
          title="Double-click to edit the text, drag to move, drag an edge to make it longer or shorter, Delete to remove"
          style:left="{pct(s.startMs)}%"
          style:width="{pct(s.endMs) - pct(s.startMs)}%"
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
          <span class="zoom-edge start" aria-hidden="true" style:width="{fit.edgePx}px"></span>
          <span class="zoom-edge end" aria-hidden="true" style:width="{fit.edgePx}px"></span>
          {#if fit.showText}
            <span class="zoom-text sub-text">{s.text || "…"}</span>
          {/if}
          {#if selectedSubtitle === i && !locked && fit.showRemove}
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

  {/if}

  {#each tracks as t, i (t.key)}
    <div class="label" class:last={i === tracks.length - 1}>
      <div class="line">
        <span class="name" title={t.name}>{t.name}</span>
        <button
          class="add-part"
          aria-label="Mute part of {t.name}"
          title="Mute part of {t.name} from the playhead (or Shift+drag on the lane)"
          disabled={locked}
          onclick={() => onpartadd(t.track)}
        >
          <Icon name="volume" size={12} />
        </button>
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
        title="Shift+drag to mute part of {t.name}"
        onpointerdown={(e) => laneDown(e, t.track)}
        onpointermove={laneMove}
        onpointerup={laneUp}
        onpointercancel={laneUp}
        onlostpointercapture={laneUp}
      >
        <canvas class="wave {t.key.startsWith('track-') ? 'track' : t.key}" class:muted={t.muted} use:waveform={{ peaks: t.peaks, ...waveShare(t) }}></canvas>
        {#each shading.dims as d, i (i)}
          <div class="dim" style:left="{d.left}%" style:width="{d.width}%"></div>
        {/each}
        {#each shading.cuts as c (c.left)}
          <div
            class="cut"
            class:open-left={c.openLeft}
            class:open-right={c.openRight}
            style:left="{c.left}%"
            style:width="{c.width}%"
          ></div>
        {/each}
        {#each partsOn(audioParts, t.track) as { part: p, i } (i)}
          {@const fit = layoutOf(p.startMs, p.endMs)}
          <!-- A pointer target for placing the part in time; Enter picks it from the keyboard. -->
          <div
            class="part-block"
            class:down={p.level > 0}
            class:compact={!fit.showText}
            class:selected={selectedPart === i}
            class:moving={partDrag?.i === i && partDrag.what === "move"}
            class:resizing={partDrag?.i === i && partDrag.what !== "move"}
            role="button"
            tabindex="0"
            aria-label="{t.name} {levelLabel(p.level).toLowerCase()} from {(p.startMs / 1000).toFixed(1)} s to {(p.endMs / 1000).toFixed(1)} s"
            aria-pressed={selectedPart === i}
            title="Drag to move, drag an edge to make it longer or shorter, Delete to remove"
            style:left="{pct(p.startMs)}%"
            style:width="{pct(p.endMs) - pct(p.startMs)}%"
            onpointerdown={(e) => partDown(e, i)}
            onpointermove={partMove}
            onpointerup={partUp}
            onpointercancel={partUp}
            onlostpointercapture={partUp}
            onkeydown={(e) => partKey(e, i)}
          >
            <span class="zoom-edge start" aria-hidden="true" style:width="{fit.edgePx}px"></span>
            <span class="zoom-edge end" aria-hidden="true" style:width="{fit.edgePx}px"></span>
            {#if fit.showText}
              {#if selectedPart === i && !locked}
                <button
                  class="part-level"
                  title="Change how far it's turned down: Muted, 25% or 50%"
                  aria-label="Level: {levelLabel(p.level)}. Change it"
                  onpointerdown={(e) => e.stopPropagation()}
                  onclick={(e) => {
                    e.stopPropagation();
                    onpartlevel(i);
                  }}
                >
                  {levelLabel(p.level)}
                </button>
              {:else}
                <span class="zoom-text part-text">{levelLabel(p.level)}</span>
              {/if}
            {/if}
            {#if selectedPart === i && !locked && fit.showRemove}
              <button
                class="zoom-remove"
                aria-label="Remove this muted part"
                title="Remove (Delete)"
                onpointerdown={(e) => e.stopPropagation()}
                onclick={(e) => {
                  e.stopPropagation();
                  onpartremove(i);
                }}
              >
                <Icon name="close" size={11} />
              </button>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/each}


  {#if zoomed}
    <div class="label scroll-label">
      <button class="btn btn-sm show-all" title="Show the whole clip (0)" onclick={() => onview(fullView(duration))}>
        Show all
      </button>
    </div>
    <div class="scroll">
      <!-- A pointer target only: the keyboard moves along the clip with the playhead. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="scroll-track"
        bind:this={scrollTrack}
        onpointerdown={scrollDown}
        onpointermove={scrollMove}
        onpointerup={scrollUp}
        onpointercancel={scrollUp}
        onlostpointercapture={scrollUp}
      >
        {#if duration > 0}
          <span class="scroll-mark" style:left="{(position / duration) * 100}%"></span>
        {/if}
        <div
          class="scroll-thumb"
          class:dragging={scrollDrag !== null}
          style:left="{thumbBox.left}%"
          style:width="{thumbBox.width}%"
        ></div>
      </div>
    </div>
  {/if}

  {#if tracks.length && inView(position, view)}
    <div class="playhead-span" style:grid-row="3 / span {tracks.length + extraRows}" aria-hidden="true">
      <div class="playhead" style:left="{pct(position)}%"></div>
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
    height: 1.6rem;
    padding: 0;
  }

  /* The timeline's zoom: − slider + in the ruler's corner. */
  .timeline-zoom {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    height: 100%;
    padding: 0 0.5rem;
  }

  .timeline-zoom .range {
    flex: 1;
    min-width: 0;
  }

  .zoom-step {
    display: grid;
    flex: none;
    place-items: center;
    width: 1.2rem;
    height: 1.2rem;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
  }

  .zoom-step:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }

  .zoom-step:disabled {
    opacity: 0.4;
    cursor: default;
  }

  /* Zoomed in: blocks running off screen are cut at the lane's edge. */
  .zoomed .lane.zoom .area {
    overflow: hidden;
  }

  /* The scrollbar under the lanes while zoomed in. */
  .scroll-label {
    padding: 0.25rem 0.5rem;
    border-top: 1px solid var(--border);
    border-bottom: none;
  }

  .show-all {
    height: 1.5rem;
    padding: 0 0.5rem;
    font-size: 0.75rem;
  }

  .scroll {
    position: relative;
    border-top: 1px solid var(--border);
  }

  .scroll-track {
    position: absolute;
    top: 50%;
    right: var(--inset);
    left: var(--inset);
    height: 0.7rem;
    border-radius: 999px;
    background: var(--surface-2);
    cursor: pointer;
    transform: translateY(-50%);
    touch-action: none;
  }

  .scroll-thumb {
    position: absolute;
    top: 0;
    bottom: 0;
    min-width: 1.5rem;
    box-sizing: border-box;
    border: 1px solid var(--accent);
    border-radius: 999px;
    background: var(--accent-soft);
    cursor: grab;
  }

  .scroll-thumb.dragging {
    cursor: grabbing;
  }

  /* Where the playhead is in the whole clip. */
  .scroll-mark {
    position: absolute;
    top: -2px;
    bottom: -2px;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: #fff;
    opacity: 0.7;
    pointer-events: none;
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
    /* Clear of the end edge, which resizes (10 px on a block wide enough for this button). */
    margin-right: calc(10px + 0.3rem);
    padding: 0;
    border: none;
    border-radius: 4px;
    background: rgba(5, 4, 10, 0.55);
    color: #fff;
    cursor: pointer;
  }

  /* Too short for its text: a slim marker that is still easy to grab. */
  .zoom-block.compact {
    border-radius: 3px;
    box-shadow:
      inset 2px 0 0 var(--accent),
      inset -2px 0 0 var(--accent);
  }
  .zoom-block.compact.selected {
    box-shadow:
      inset 2px 0 0 #fff,
      inset -2px 0 0 #fff,
      0 0 0 1px var(--accent);
  }

  .locked .zoom-block,
  .locked .part-block {
    cursor: not-allowed;
  }

  /* A muted part covers its stretch of the waveform, which shows faintly through hatching; a part
     turned down covers it less. */
  .part-block {
    position: absolute;
    top: 0.25rem;
    bottom: 0.25rem;
    z-index: 1;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    overflow: hidden;
    box-sizing: border-box;
    min-width: 6px;
    padding-top: 0.15rem;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background:
      repeating-linear-gradient(-45deg, rgba(255, 255, 255, 0.07) 0 2px, transparent 2px 8px),
      rgba(11, 10, 16, 0.85);
    color: var(--text);
    cursor: grab;
    touch-action: none;
  }
  .part-block.down {
    background:
      repeating-linear-gradient(-45deg, rgba(255, 255, 255, 0.07) 0 2px, transparent 2px 8px),
      rgba(11, 10, 16, 0.5);
  }
  .part-block.moving {
    cursor: grabbing;
  }
  .part-block.resizing {
    cursor: ew-resize;
  }
  .part-block.selected {
    border-color: #fff;
    box-shadow: 0 0 0 1px var(--accent);
  }
  .part-block:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .part-block.compact {
    border-radius: 3px;
  }
  .part-text {
    flex: none;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.8);
  }
  .part-level {
    flex: none;
    margin-left: 0.3rem;
    padding: 0 0.35rem;
    border: none;
    border-radius: 4px;
    background: rgba(5, 4, 10, 0.7);
    color: #fff;
    font-size: 0.72rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    line-height: 1.1rem;
    cursor: pointer;
  }
  .part-block .zoom-remove {
    margin-left: auto;
  }

  .add-part {
    display: grid;
    flex: none;
    place-items: center;
    width: 1.25rem;
    height: 1.25rem;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .add-part:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--accent);
  }
  .add-part:disabled {
    opacity: 0.45;
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
