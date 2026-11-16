<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Piece } from "../lib/cuts";
  import { laneShading, rulerTicks } from "../lib/lanes";
  import { msToX, xToMs, type TrimRange } from "../lib/trim";
  import { peaks as bucketPeaks } from "../lib/waveform";
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
    master,
    blur,
    locked = false,
    onvolume,
    onmute,
    onmaster,
    onblur,
    onseek,
    onscrubstart,
    onscrubend,
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
    /** The master volume, 0–100. */
    master: number;
    /** "Blur video" is on. */
    blur: boolean;
    /** The edit can't change now (the clip is uploading): only the playhead moves. */
    locked?: boolean;
    onvolume: (key: string, volume: number) => void;
    onmute: (key: string) => void;
    onmaster: (volume: number) => void;
    onblur: (blur: boolean) => void;
    /** The user clicked or dragged on a lane to jump to `ms`. */
    onseek: (ms: number) => void;
    /** The pointer went down on a lane. */
    onscrubstart?: () => void;
    /** That pointer was released, or the browser took it away. */
    onscrubend?: () => void;
    /** The video lane: the TrimBar. */
    video: Snippet;
  } = $props();

  const ticks = $derived(rulerTicks(duration));
  const shading = $derived(laneShading(duration, trim, pieces));
  /** Ruler, video, one row per track, master: the playhead spans the audio rows. */
  const rows = $derived(["auto", "auto", ...tracks.map(() => "auto"), "auto"].join(" "));

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

  {#each tracks as t (t.key)}
    <div class="label">
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
    <div class="lane audio">
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

  <div class="label master">
    <div class="line">
      <span class="name">Master</span>
      <output>{level(master)}</output>
    </div>
    <div class="line">
      <span class="mute-space"></span>
      <input
        class="range"
        type="range"
        min="0"
        max="100"
        step="1"
        value={master}
        disabled={locked}
        style:--fill="{master}%"
        aria-label="Master volume"
        title="The whole clip: every track together"
        oninput={(e) => onmaster(Number(e.currentTarget.value))}
      />
    </div>
  </div>
  <div class="lane master"></div>

  {#if tracks.length}
    <div class="playhead-span" style:grid-row="3 / span {tracks.length}" aria-hidden="true">
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

  .lane.master,
  .label.master {
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
