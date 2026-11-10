<script lang="ts">
  import { onMount } from "svelte";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api, errorMessage, on, type ClipView, type EditInput, type EditorData } from "../lib/api";
  import { exportFileName } from "../lib/format";
  import { clampTrim, formatClock, savedTrim, type TrimRange } from "../lib/trim";
  import { toast } from "../lib/toasts.svelte";
  import { isBusy, uploads } from "../lib/uploads.svelte";
  import Icon from "../components/Icon.svelte";
  import TrimBar from "../components/TrimBar.svelte";
  import UploadDialog from "../components/UploadDialog.svelte";

  let {
    clip,
    onclose,
    dirty = $bindable(false),
  }: {
    clip: ClipView;
    /** Back to the library (the app asks about unsaved changes). */
    onclose: () => void;
    /** Whether there are unsaved changes, for the app's "Discard changes?" guard. */
    dirty?: boolean;
  } = $props();

  /** Audio more than this far off the video is put back in step. */
  const MAX_DRIFT_S = 0.05;
  const DRIFT_CHECK_MS = 250;

  let data = $state<EditorData | null>(null);
  let loadError = $state<string | null>(null);
  let mediaError = $state(false);

  let duration = $state(0);
  let trim = $state<TrimRange>({ start: 0, end: 0 });
  let gameVolume = $state(100);
  let micVolume = $state(100);
  /** The edit as last loaded or saved; `dirty` compares against it. */
  let stored = $state<EditInput | null>(null);

  let video = $state<HTMLVideoElement>();
  let gameAudio = $state<HTMLAudioElement>();
  let micAudio = $state<HTMLAudioElement>();
  let playing = $state(false);
  let position = $state(0);

  let saving = $state(false);
  let exportProgress = $state<number | null>(null);
  let uploadOpen = $state(false);

  const job = $derived(uploads[clip.id]);
  const uploading = $derived(isBusy(job));
  const exporting = $derived(exportProgress !== null);
  const busy = $derived(saving || exporting || uploading);

  /** The edit as the UI shows it now, in the shape it is saved. */
  const edit = $derived.by((): EditInput => {
    const t = savedTrim(trim, duration);
    return { trimStartMs: t.start, trimEndMs: t.end, gameVolume: gameVolume / 100, micVolume: micVolume / 100 };
  });

  const changed = $derived(
    !!stored &&
      (edit.trimStartMs !== stored.trimStartMs ||
        edit.trimEndMs !== stored.trimEndMs ||
        edit.gameVolume !== stored.gameVolume ||
        edit.micVolume !== stored.micVolume),
  );

  $effect(() => {
    dirty = changed;
  });

  /** A stored gain (0.0–2.0 in the library) as a 0–100% slider value. */
  function percent(gain: number): number {
    return Number.isFinite(gain) ? Math.round(Math.min(1, Math.max(0, gain)) * 100) : 100;
  }

  async function load() {
    loadError = null;
    try {
      const d = await api.openEditor(clip.id);
      duration = d.durationMs;
      trim = clampTrim(d.clip.trimStartMs ?? 0, d.clip.trimEndMs ?? d.durationMs, d.durationMs);
      gameVolume = percent(d.clip.gameVolume);
      micVolume = percent(d.clip.micVolume);
      position = trim.start;
      data = d;
      stored = edit;
    } catch (e) {
      loadError = errorMessage(e);
    }
  }

  onMount(() => {
    void load();

    // Keep the audio on the video's clock and the playhead moving.
    const drift = setInterval(() => {
      if (!video || video.paused || video.seeking) return;
      for (const audio of audios()) {
        if (!audio.seeking && Math.abs(audio.currentTime - video.currentTime) > MAX_DRIFT_S) {
          audio.currentTime = video.currentTime;
        }
      }
    }, DRIFT_CHECK_MS);

    let frame = requestAnimationFrame(function tick() {
      if (video && !video.seeking) {
        const ms = video.currentTime * 1000;
        position = ms;
        // Playback loops inside the trim.
        if (!video.paused && (ms >= trim.end || ms < trim.start - 50)) seek(trim.start);
      }
      frame = requestAnimationFrame(tick);
    });

    const stopProgress = on("export-progress", ({ id, progress }) => {
      if (id === clip.id && exportProgress !== null) exportProgress = progress;
    });

    return () => {
      clearInterval(drift);
      cancelAnimationFrame(frame);
      stopProgress();
      video?.pause();
      for (const audio of audios()) audio.pause();
    };
  });

  // Playback gain: the sliders' 0–100% is HTMLMediaElement's 0.0–1.0.
  $effect(() => {
    if (gameAudio) gameAudio.volume = gameVolume / 100;
  });
  $effect(() => {
    if (micAudio) micAudio.volume = micVolume / 100;
  });

  function audios(): HTMLAudioElement[] {
    return [gameAudio, micAudio].filter((a): a is HTMLAudioElement => !!a);
  }

  function syncAudio() {
    if (!video) return;
    for (const audio of audios()) audio.currentTime = video.currentTime;
  }

  function playAudio() {
    for (const audio of audios()) audio.play().catch(() => {});
  }

  function pauseAudio() {
    for (const audio of audios()) audio.pause();
  }

  // While dragging, only one seek runs at a time; the newest target waits in `pendingSeek` and
  // is applied when the current seek lands. Each seek first freezes the last frame on a canvas
  // overlay, so WebKitGTK's black frame during decoding never shows.
  let freeze = $state<HTMLCanvasElement>();
  let frozen = $state(false);
  let pendingSeek: number | null = null;

  function freezeFrame() {
    if (!video || !freeze || frozen || video.videoWidth === 0) return;
    freeze.width = video.videoWidth;
    freeze.height = video.videoHeight;
    try {
      freeze.getContext("2d")?.drawImage(video, 0, 0);
      frozen = true;
    } catch {
      // No snapshot available: just seek.
    }
  }

  let unfreezeTimer: ReturnType<typeof setTimeout> | undefined;
  function unfreezeAfterPaint() {
    clearTimeout(unfreezeTimer);
    const done = () => {
      clearTimeout(unfreezeTimer);
      // Another seek may have started meanwhile; keep the snapshot up for it.
      if (!video?.seeking) frozen = false;
    };
    const v = video as (HTMLVideoElement & { requestVideoFrameCallback?: (cb: () => void) => number }) | undefined;
    v?.requestVideoFrameCallback?.(() => requestAnimationFrame(done));
    unfreezeTimer = setTimeout(done, 250);
  }

  function seek(ms: number) {
    if (!video) return;
    position = ms;
    if (video.seeking) {
      pendingSeek = ms;
      return;
    }
    freezeFrame();
    video.currentTime = ms / 1000;
  }

  function togglePlay() {
    if (!video || mediaError) return;
    if (video.paused) {
      const ms = video.currentTime * 1000;
      if (ms < trim.start || ms >= trim.end - 50) seek(trim.start);
      video.play().catch(() => {});
    } else {
      video.pause();
    }
  }

  // The video drives; the two audio elements follow it. During a seek the audio is paused and
  // only resumes once the video landed, so the tracks don't drift apart on every jump.
  function onplay() {
    playing = true;
    syncAudio();
    playAudio();
  }

  function onpause() {
    playing = false;
    pauseAudio();
  }

  function onseeked() {
    if (pendingSeek !== null && video) {
      const next = pendingSeek;
      pendingSeek = null;
      video.currentTime = next / 1000;
      return;
    }
    // Unfreeze only once the new frame is actually on screen (WebKitGTK paints it after
    // `seeked`); fall back to a short delay where requestVideoFrameCallback is missing.
    unfreezeAfterPaint();
    syncAudio();
    if (video && !video.paused) playAudio();
  }

  function onended() {
    // The trim runs to the clip's end: loop from the start.
    seek(trim.start);
    video?.play().catch(() => {});
  }

  function onhandlemove(_handle: "start" | "end", ms: number) {
    // Show the frame under the handle while paused; while playing, the loop takes care of it.
    if (video?.paused) seek(ms);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== " " || e.defaultPrevented || uploadOpen) return;
    const target = e.target as HTMLElement | null;
    // Buttons and form fields use the space bar themselves.
    if (target?.closest("button, input, textarea, select, [contenteditable], [role='dialog']")) return;
    e.preventDefault();
    togglePlay();
  }

  async function saveEdit(): Promise<boolean> {
    if (!stored || !changed) return true;
    saving = true;
    const next = edit;
    try {
      await api.saveEdit(clip.id, next);
      stored = next;
      if (data) {
        data.clip = {
          ...data.clip,
          trimStartMs: next.trimStartMs,
          trimEndMs: next.trimEndMs,
          gameVolume: next.gameVolume,
          micVolume: next.micVolume,
          uploadId: null,
          uploadOffset: null,
        };
      }
      return true;
    } catch (e) {
      toast(errorMessage(e), "error");
      return false;
    } finally {
      saving = false;
    }
  }

  async function onsave() {
    if (await saveEdit()) toast("Edit saved", "success");
  }

  async function onexport() {
    if (!data) return;
    let dest: string | null;
    try {
      dest = await saveDialog({
        title: "Export a copy",
        defaultPath: exportFileName(data.clip.title),
        filters: [{ name: "MP4 video", extensions: ["mp4"] }],
      });
    } catch (e) {
      toast(errorMessage(e), "error");
      return;
    }
    if (!dest) return;
    if (!/\.mp4$/i.test(dest)) dest += ".mp4";
    exportProgress = 0;
    try {
      await api.exportCopy(clip.id, dest, edit);
      toast("Copy exported", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      exportProgress = null;
    }
  }

  async function onupload() {
    // The upload exports the saved edit, so save first: it uploads the cut the user sees.
    if (await saveEdit()) {
      video?.pause();
      uploadOpen = true;
    }
  }
</script>

<svelte:window {onkeydown} />

<section class="editor">
  <header class="bar">
    <button class="btn btn-ghost back" onclick={onclose}>
      <Icon name="back" />
      Clips
    </button>
    <div class="title">
      <h1 title={data?.clip.title ?? clip.title}>{data?.clip.title ?? clip.title}</h1>
      <span class="muted" class:unknown={!clip.gameName}>{clip.gameName ?? "Unknown game"}</span>
    </div>
    {#if changed}<span class="unsaved">Unsaved changes</span>{/if}
  </header>

  {#if loadError}
    <div class="center">
      <Icon name="alert" size={32} />
      <p>{loadError}</p>
      <div class="row">
        <button class="btn" onclick={onclose}>Back to clips</button>
        <button class="btn btn-primary" onclick={load}><Icon name="retry" size={16} /> Try again</button>
      </div>
    </div>
  {:else if !data}
    <div class="center">
      <span class="spinner"></span>
      <p>Preparing the clip…</p>
    </div>
  {:else}
    <div class="stage">
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        bind:this={video}
        src={data.videoUrl}
        crossorigin="anonymous"
        muted
        playsinline
        preload="auto"
        onplay={onplay}
        onpause={onpause}
        onseeking={pauseAudio}
        onseeked={onseeked}
        onended={onended}
        onerror={() => (mediaError = true)}
        onclick={togglePlay}
      ></video>
      <canvas bind:this={freeze} class="freeze" class:show={frozen} aria-hidden="true"></canvas>
      {#if mediaError}
        <div class="media-error">
          <Icon name="alert" size={28} />
          <p>Klipt can't play this clip here. You can still trim, export and upload it.</p>
        </div>
      {/if}
      <audio bind:this={gameAudio} src={data.gameAudioUrl} preload="auto"></audio>
      {#if data.micAudioUrl}
        <audio bind:this={micAudio} src={data.micAudioUrl} preload="auto"></audio>
      {/if}
    </div>

    <div class="timeline">
      <div class="transport">
        <button
          class="btn btn-primary btn-icon play"
          onclick={togglePlay}
          disabled={mediaError}
          aria-label={playing ? "Pause" : "Play"}
          title={playing ? "Pause (Space)" : "Play (Space)"}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            {#if playing}
              <path d="M7 5h3.5v14H7zM13.5 5H17v14h-3.5z" />
            {:else}
              <path d="M8 5.5v13a.8.8 0 0 0 1.2.7l10.4-6.5a.8.8 0 0 0 0-1.4L9.2 4.8A.8.8 0 0 0 8 5.5z" />
            {/if}
          </svg>
        </button>
        <span class="clock"><strong>{formatClock(position)}</strong> / {formatClock(duration)}</span>
      </div>
      <TrimBar {duration} bind:start={trim.start} bind:end={trim.end} {position} onseek={seek} {onhandlemove} />
    </div>

    <div class="panel">
      <div class="mixer">
        <label class="volume">
          <span>Game</span>
          <input type="range" min="0" max="100" step="1" bind:value={gameVolume} aria-label="Game volume" />
          <output>{gameVolume}%</output>
        </label>
        {#if data.micAudioPath}
          <label class="volume">
            <span>Mic</span>
            <input type="range" min="0" max="100" step="1" bind:value={micVolume} aria-label="Mic volume" />
            <output>{micVolume}%</output>
          </label>
        {/if}
      </div>

      <div class="actions">
        {#if uploading}
          <p class="note muted">Uploading — you can change this clip when it's done.</p>
        {:else if exporting}
          <div class="exporting">
            <span>Exporting… {Math.round((exportProgress ?? 0) * 100)}%</span>
            <div class="progress" style:--value={exportProgress ?? 0}><div></div></div>
          </div>
        {/if}
        <button class="btn" onclick={onsave} disabled={!changed || busy}>
          {#if saving}<span class="spinner"></span>{:else}<Icon name="check" size={16} />{/if}
          Save edit
        </button>
        <button class="btn" onclick={onexport} disabled={busy}>
          <Icon name="film" size={16} /> Export copy…
        </button>
        <button class="btn btn-primary" onclick={onupload} disabled={busy}>
          <Icon name="upload" size={16} /> Upload
        </button>
      </div>
    </div>
  {/if}
</section>

{#if uploadOpen && data}
  <UploadDialog clip={data.clip} onclose={() => (uploadOpen = false)} />
{/if}

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    height: 100%;
    padding: 1rem 1.75rem 1.5rem;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    min-width: 0;
  }

  .back {
    flex: none;
    padding-left: 0.5rem;
  }

  .title {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
    min-width: 0;
  }

  h1 {
    overflow: hidden;
    font-size: 1.25rem;
    letter-spacing: -0.01em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title .muted {
    flex: none;
    font-size: 0.9rem;
  }

  .unknown {
    font-style: italic;
  }

  .unsaved {
    flex: none;
    margin-left: auto;
    padding: 0.2rem 0.6rem;
    border-radius: 999px;
    background: rgba(255, 181, 71, 0.14);
    color: var(--warning);
    font-size: 0.8rem;
    font-weight: 650;
  }

  .center {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.9rem;
    color: var(--text-muted);
    text-align: center;
  }

  .row {
    display: flex;
    gap: 0.5rem;
  }

  .stage {
    position: relative;
    display: grid;
    flex: 1;
    place-items: center;
    min-height: 12rem;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: #000;
  }

  .freeze {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
    visibility: hidden;
  }
  .freeze.show {
    visibility: visible;
  }
  video {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    cursor: pointer;
  }

  .media-error {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.6rem;
    max-width: 24rem;
    color: var(--warning);
    text-align: center;
  }

  .media-error p {
    color: var(--text-muted);
  }

  .timeline {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    flex: none;
  }

  .transport {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.35rem;
    flex: none;
    width: 7.5rem;
  }

  .play {
    width: 3.25rem;
    height: 3.25rem;
    border-radius: 50%;
  }

  .clock {
    color: var(--text-muted);
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .clock strong {
    color: var(--text);
    font-weight: 650;
  }

  .timeline :global(.trimbar) {
    flex: 1;
    min-width: 0;
  }

  .panel {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 1rem 2rem;
    flex: none;
    padding: 0.9rem 1rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }

  .mixer {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 2rem;
  }

  .volume {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    font-weight: 600;
  }

  .volume span {
    width: 2.6rem;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .volume input {
    width: 11rem;
    accent-color: var(--accent);
    cursor: pointer;
  }

  output {
    width: 2.8rem;
    font-size: 0.88rem;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-left: auto;
  }

  .note {
    font-size: 0.87rem;
  }

  .exporting {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    width: 10rem;
    font-size: 0.82rem;
    font-weight: 600;
  }
</style>
