<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api, errorMessage, on, type ClipView, type EditInput, type EditorData, type GameCount } from "../lib/api";
  import type { RemoveWhere } from "../lib/removal";
  import { AudioEngine } from "../lib/audioEngine";
  import {
    cutSeekAhead,
    schedule,
    SEEK_LATENCY_START_MS,
    seekLatency,
    videoCorrection,
    videoLedJump,
    type Segment,
    audioStartMs,
    videoStarted,
  } from "../lib/playSchedule";
  import { exportFileName, formatSize, hueOf } from "../lib/format";
  import { editorStep, type EditorProgress } from "../lib/progress";
  import { frameUrl, previewIndex } from "../lib/filmstrip";
  import { clampTrim, formatClock, savedTrim, type TrimRange } from "../lib/trim";
  import {
    clampToKept,
    keptLength,
    keptRanges,
    piecesFromRanges,
    playFrom,
    removedRanges,
    sameRanges,
    splitAt,
    wholeClip,
    type Piece,
  } from "../lib/cuts";
  import { formatTenths, stepMs } from "../lib/timecode";
  import { addZoomAt, removeZoom, sameZooms, setZoomBox, zoomAt, zoomIndexAt } from "../lib/zoom";
  import type { SubtitleRange, ZoomRange } from "../lib/api";
  import { addSubtitleAt, removeSubtitle, sameSubtitles, savedSubtitles, subtitleIndexAt } from "../lib/subtitles";
  import { toggleMute } from "../lib/lanes";
  import { menu } from "../lib/a11y";
  import { Autosave, type SaveStatus } from "../lib/autosave";
  import { EditHistory } from "../lib/editHistory";
  import { Gestures } from "../lib/gestures";
  import { editorKeyAction, keyTarget, RELEASES_FOCUS } from "../lib/editorKeys";
  import { toast } from "../lib/toasts.svelte";
  import { isBusy, uploads } from "../lib/uploads.svelte";
  import DeleteClipDialog from "../components/DeleteClipDialog.svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import TrackLanes from "../components/TrackLanes.svelte";
  import TrimBar from "../components/TrimBar.svelte";
  import UploadDialog from "../components/UploadDialog.svelte";
  import ZoomBox from "../components/ZoomBox.svelte";

  let {
    clip,
    onclose,
    ondeleted,
  }: {
    clip: ClipView;
    /** Back to the library (a change not saved yet is saved on the way out). */
    onclose: () => void;
    /** The clip was deleted: back to the library. */
    ondeleted: () => void;
  } = $props();

  /** Plays every audio track on one clock; the muted video follows it. */
  const engine = new AudioEngine();
  /** The tracks are decoded (or failed): playback can start and the sliders apply. */
  let audioReady = $state(false);
  /** The audio engine can't play at all: the video plays without sound, on its own clock. */
  let silent = false;
  /** The editor was closed: a load finishing late must not toast. */
  let closed = false;

  let data = $state<EditorData | null>(null);
  /** How far the core got preparing the clip, while `data` isn't here yet. */
  let preparing = $state<EditorProgress | null>(null);
  const prepStep = $derived(editorStep(preparing));
  /** Audio bytes loaded so far, until the audio is ready; `total` null while unknown. */
  let audioLoad = $state<{ loaded: number; total: number | null } | null>(null);
  let loadError = $state<string | null>(null);
  let mediaError = $state(false);

  let duration = $state(0);
  let trim = $state<TrimRange>({ start: 0, end: 0 });
  /** The timeline's kept and removed pieces (the split lines are their boundaries). */
  let pieces = $state<Piece[]>(wholeClip(0));
  /** The parts that play and get uploaded: the trim minus the removed pieces. */
  const kept = $derived(keptRanges(pieces, trim));
  /** What plays: the kept parts, or the trim while nothing is kept (as before, so it can be seen). */
  const playRanges = $derived(kept.length ? kept : [trim]);
  let gameVolume = $state(100);
  let micVolume = $state(100);
  let voiceVolume = $state(100);
  let masterVolume = $state(100);
  /** The other audio tracks' sliders (0–100), by track index. */
  let extraVolumes = $state<Record<number, number>>({});
  /** A muted track's level before it was muted, by engine key (editor UI only, not saved). */
  let unmuteLevels = $state<Record<string, number>>({});
  /** "Blur video": uploads and exports blur the picture (a clip where only the sound matters). */
  let blurVideo = $state(false);
  /** The zooms (sorted, never overlapping), in ms into the file. */
  let zooms = $state<ZoomRange[]>([]);
  /** The zoom picked on the Zoom lane: its box can be placed on the video. */
  let selectedZoom = $state<number | null>(null);
  /** The subtitles (sorted, never overlapping), in ms into the file. */
  let subtitles = $state<SubtitleRange[]>([]);
  /** The subtitle picked on the Subtitles lane. */
  let selectedSubtitle = $state<number | null>(null);
  /** The subtitle whose text is being typed (its text box is open on the video). */
  let editingSubtitle = $state<number | null>(null);
  /** The thumbnail's frame; null = automatic. */
  let thumb = $state<number | null>(null);
  /** The edit as last loaded or saved; `changed` compares against it. */
  let stored = $state<EditInput | null>(null);
  let storedThumb = $state<number | null>(null);

  let video = $state<HTMLVideoElement>();

  // The preview of "Blur video": as strong as the export's blur (sigma 8 at up to 720p, see
  // export.rs), scaled to the size the picture is shown at.
  const BLUR_SIGMA = 8;
  let stageWidth = $state(0);
  let stageHeight = $state(0);
  let videoSize = $state<{ width: number; height: number } | null>(null);
  const previewBlur = $derived.by(() => {
    if (!blurVideo || !videoSize || !stageWidth || !stageHeight) return undefined;
    const shown = Math.min(stageHeight, (stageWidth * videoSize.height) / videoSize.width);
    return `blur(${((BLUR_SIGMA * shown) / Math.min(720, videoSize.height)).toFixed(1)}px)`;
  });
  /** Where the picture shows on the stage (inside the letterbox), in stage pixels. */
  const videoRect = $derived.by(() => {
    if (!videoSize || !videoSize.width || !videoSize.height || !stageWidth || !stageHeight) {
      return { left: 0, top: 0, width: stageWidth, height: stageHeight };
    }
    const fit = Math.min(stageWidth / videoSize.width, stageHeight / videoSize.height);
    const width = videoSize.width * fit;
    const height = videoSize.height * fit;
    return { left: (stageWidth - width) / 2, top: (stageHeight - height) / 2, width, height };
  });
  let playing = $state(false);
  let position = $state(0);
  /**
   * The picked zoom's box is up on the video: paused inside that zoom. The picture then shows
   * unzoomed, so the box can go anywhere on it.
   */
  const placingZoom = $derived(
    !playing && selectedZoom !== null && zoomIndexAt(zooms, position) === selectedZoom,
  );
  /** How the preview is zoomed at the playhead (the same easing as the export). */
  const view = $derived(placingZoom ? { scale: 1, x: 0.5, y: 0.5 } : zoomAt(zooms, position));
  /** The subtitle showing at the playhead (hidden while its text is being typed). */
  const shownSubtitle = $derived.by(() => {
    const i = subtitleIndexAt(subtitles, position);
    return i < 0 || i === editingSubtitle ? null : subtitles[i];
  });

  /** How many filmstrip pictures of this clip are on disk (0 once a picture fails to load). */
  let stripFrames = $state(0);
  /** The pointer is down on the timeline: playback is paused and moves show filmstrip pictures. */
  let scrubbing = false;
  /** Playback was on when the scrub started: it goes on when the pointer is released. */
  let wasPlaying = false;
  /** The filmstrip picture shown over the video while scrubbing; null = the video shows. */
  let scrubFrame = $state<string | null>(null);
  /** The picture shown is ahead of the video: it seeks there once the scrub rests or ends. */
  let scrubAhead = false;
  let settleTimer: ReturnType<typeof setTimeout> | undefined;
  /** How long the pointer rests before the video seeks to the exact frame under it. */
  const SCRUB_SETTLE_MS = 150;

  /** The header's save status. */
  let saveStatus = $state<SaveStatus>("saved");
  /** The ⋯ menu (Show in folder, Delete) is open. */
  let moreOpen = $state(false);
  // svelte-ignore state_referenced_locally
  let gameName = $state(clip.gameName);
  /** "Set game": the name being typed and the games Klipt knows (for suggestions). */
  let tagging = $state<{ value: string; games: GameCount[] } | null>(null);
  /** "Delete clip?": the clip as the library has it now (it may have been uploaded meanwhile). */
  let removing = $state<ClipView | null>(null);
  let deleting = $state(false);
  let exportProgress = $state<number | null>(null);
  let uploadOpen = $state(false);

  const job = $derived(uploads[clip.id]);
  const uploading = $derived(isBusy(job));
  const exporting = $derived(exportProgress !== null);
  const busy = $derived(exporting || uploading);
  /**
   * The clip can't be saved while it uploads or exports (both hold it busy): the edit is locked
   * until they end.
   */
  const editLocked = $derived(uploading || exporting);
  const nothingKept = "Keep at least part of the clip";

  // An upload saves its title as the clip's: show it here at once, not on the next open.
  $effect(() => {
    if (job?.status === "done" && data && data.clip.title !== job.title) {
      data.clip = { ...data.clip, title: job.title };
    }
  });

  /** The edit as the UI shows it now, in the shape it is saved. */
  const edit = $derived.by((): EditInput => {
    const t = savedTrim(trim, duration);
    return {
      trimStartMs: t.start,
      trimEndMs: t.end,
      gameVolume: gameVolume / 100,
      micVolume: micVolume / 100,
      voiceVolume: voiceVolume / 100,
      masterVolume: masterVolume / 100,
      extraVolumes: Object.fromEntries(Object.entries(extraVolumes).map(([track, v]) => [track, v / 100])),
      blurVideo,
      removedRanges: removedRanges(pieces, trim),
      zoomRanges: zooms.map((z) => ({ startMs: z.startMs, endMs: z.endMs, zoom: z.zoom, x: z.x, y: z.y })),
      subtitles: savedSubtitles(subtitles),
    };
  });

  /** The thumbnail frame as it is saved: on a kept part. */
  const savedThumb = $derived(thumb === null ? null : clampToKept(thumb, kept));

  function sameVolumes(a: Record<number, number>, b: Record<number, number>): boolean {
    const keys = new Set([...Object.keys(a), ...Object.keys(b)].map(Number));
    return [...keys].every((k) => (a[k] ?? 1) === (b[k] ?? 1));
  }

  const changed = $derived(
    !!stored &&
      (edit.trimStartMs !== stored.trimStartMs ||
        edit.trimEndMs !== stored.trimEndMs ||
        edit.gameVolume !== stored.gameVolume ||
        edit.micVolume !== stored.micVolume ||
        edit.voiceVolume !== stored.voiceVolume ||
        edit.masterVolume !== stored.masterVolume ||
        !sameVolumes(edit.extraVolumes, stored.extraVolumes) ||
        edit.blurVideo !== stored.blurVideo ||
        !sameRanges(edit.removedRanges, stored.removedRanges) ||
        !sameZooms(edit.zoomRanges, stored.zoomRanges) ||
        !sameSubtitles(edit.subtitles, stored.subtitles) ||
        savedThumb !== storedThumb),
  );

  /** What undo and redo bring back: everything the editor saves. */
  interface EditState {
    trim: TrimRange;
    pieces: Piece[];
    gameVolume: number;
    micVolume: number;
    voiceVolume: number;
    masterVolume: number;
    extraVolumes: Record<number, number>;
    blurVideo: boolean;
    zooms: ZoomRange[];
    subtitles: SubtitleRange[];
    thumb: number | null;
  }

  const editState = $derived<EditState>({
    trim: { start: trim.start, end: trim.end },
    pieces: pieces.map((p) => ({ start: p.start, end: p.end, removed: p.removed })),
    gameVolume,
    micVolume,
    voiceVolume,
    masterVolume,
    extraVolumes: { ...extraVolumes },
    blurVideo,
    zooms: zooms.map((z) => ({ ...z })),
    subtitles: subtitles.map((t) => ({ ...t })),
    thumb,
  });

  /** The edits made since the editor opened (created once the clip is loaded). */
  let history: EditHistory<EditState> | null = null;
  /**
   * A drag on the timeline or a volume slider in progress: its changes become one undo step,
   * recorded when it ends (so a drag that rests is still one step).
   */
  const gestures = new Gestures<"scrub" | "slider" | "zoom" | "subtitle">(() => history?.record(editState));

  function applyState(s: EditState) {
    trim = s.trim;
    pieces = s.pieces;
    gameVolume = s.gameVolume;
    micVolume = s.micVolume;
    voiceVolume = s.voiceVolume;
    masterVolume = s.masterVolume;
    extraVolumes = s.extraVolumes;
    blurVideo = s.blurVideo;
    zooms = s.zooms;
    if (selectedZoom !== null && selectedZoom >= zooms.length) selectedZoom = null;
    subtitles = s.subtitles;
    if (selectedSubtitle !== null && selectedSubtitle >= subtitles.length) selectedSubtitle = null;
    editingSubtitle = null;
    thumb = s.thumb;
  }

  function undo() {
    if (editLocked) return;
    const s = history?.undo();
    if (s) applyState(s);
  }

  function redo() {
    if (editLocked) return;
    const s = history?.redo();
    if (s) applyState(s);
  }

  /** The clip was deleted here: nothing is saved on the way out. */
  let deleted = false;
  /** The edit as it is now, for the save on the way out (kept up to date by the effect below). */
  let latest: { edit: EditInput; thumb: number | null; saveable: boolean } | null = null;

  const autosave = new Autosave({
    save: writeEdit,
    dirty: () => changed && kept.length > 0,
    // The clip can't be saved while it uploads or exports: the change waits for that to end.
    blocked: () => editLocked,
    onstatus: (status, error) => {
      saveStatus = status;
      if (status !== "failed") return;
      if (closed) leaveSaveFailed(error);
      else toast(`Couldn't save: ${errorMessage(error)}`, "error");
    },
  });

  /** A save failed after the editor closed: toasts are app-wide, so the library shows it. */
  function leaveSaveFailed(error: unknown) {
    console.error("Couldn't save the edit on leaving", error);
    toast(`Couldn't save your changes to ${data?.clip.title ?? clip.title}`, "error");
  }

  // The upload or export ended: save what is still waiting.
  $effect(() => {
    if (!editLocked) {
      untrack(() => {
        if (autosave.pending) void autosave.flush();
      });
    }
  });

  // Every change to the edit is an undo step (a drag is one) and is saved a moment later. An
  // undo or redo is a change too: the history skips the state it already holds, so applying it
  // doesn't add a step.
  $effect(() => {
    const state = editState;
    untrack(() => {
      if (!history) return;
      // Separate edits are separate steps; a drag is recorded once, when it ends.
      if (!gestures.active) history.record(state);
      latest = { edit, thumb: savedThumb, saveable: kept.length > 0 };
      if (changed || autosave.pending) autosave.changed();
    });
  });

  /** A stored gain (0.0–2.0 in the library) as a 0–100% slider value. */
  function percent(gain: number): number {
    return Number.isFinite(gain) ? Math.round(Math.min(1, Math.max(0, gain)) * 100) : 100;
  }

  async function load() {
    loadError = null;
    preparing = null;
    try {
      const d = await api.openEditor(clip.id);
      duration = d.durationMs;
      trim = clampTrim(d.clip.trimStartMs ?? 0, d.clip.trimEndMs ?? d.durationMs, d.durationMs);
      pieces = piecesFromRanges(d.clip.removedRanges ?? [], d.durationMs);
      gameVolume = percent(d.clip.gameVolume);
      micVolume = percent(d.clip.micVolume);
      voiceVolume = percent(d.clip.voiceVolume ?? 1);
      masterVolume = percent(d.clip.masterVolume ?? 1);
      extraVolumes = Object.fromEntries(d.extraTracks.map((t) => [t.index, percent(d.clip.extraVolumes?.[t.index] ?? 1)]));
      blurVideo = d.clip.blurVideo ?? false;
      zooms = d.clip.zoomRanges ?? [];
      subtitles = d.clip.subtitles ?? [];
      thumb = d.clip.thumbnailMs;
      position = trim.start;
      stripFrames = d.filmstrip.frames;
      data = d;
      stored = edit;
      storedThumb = savedThumb;
      history = new EditHistory(editState, 0);
      void loadAudio(d);
    } catch (e) {
      loadError = errorMessage(e);
    }
  }

  /**
   * The engine's key for each track, with the name the "couldn't load" toast uses and its lane's
   * label.
   */
  function audioTracks(d: EditorData): { key: string; url: string; name: string; label: string }[] {
    return [
      // An imported clip with more tracks: Klipt can't know which one is the game.
      { key: "game", url: d.gameAudioUrl, name: "game", label: d.extraTracks.length > 0 ? "Track 1" : "Game" },
      ...(d.micAudioUrl ? [{ key: "mic", url: d.micAudioUrl, name: "microphone", label: "Mic" }] : []),
      ...(d.voiceAudioUrl ? [{ key: "voice", url: d.voiceAudioUrl, name: "voice chat", label: "Voice chat" }] : []),
      ...d.extraTracks.map((t) => ({
        key: `track-${t.index}`,
        url: t.audioUrl,
        name: `track ${t.index + 1}`,
        label: `Track ${t.index + 1}`,
      })),
    ];
  }

  /** How many waveform buckets the lanes get (each lane fits them to its width in pixels). */
  const PEAK_BUCKETS = 4096;
  /** Each track's waveform, once the audio is decoded (null for a track that failed). */
  const lanePeaks = $derived<Record<string, Float32Array | null>>(
    audioReady && data ? Object.fromEntries(audioTracks(data).map((t) => [t.key, engine.peaks(t.key, PEAK_BUCKETS)])) : {},
  );

  /** A track's slider (0–100) by its engine key. */
  function volumeOf(key: string): number {
    if (key === "game") return gameVolume;
    if (key === "mic") return micVolume;
    if (key === "voice") return voiceVolume;
    return extraVolumes[Number(key.slice("track-".length))] ?? 100;
  }

  function setVolume(key: string, volume: number) {
    if (key === "game") gameVolume = volume;
    else if (key === "mic") micVolume = volume;
    else if (key === "voice") voiceVolume = volume;
    else extraVolumes[Number(key.slice("track-".length))] = volume;
  }

  const laneTracks = $derived(
    data
      ? audioTracks(data).map((t) => {
          const volume = volumeOf(t.key);
          return { key: t.key, name: t.label, peaks: lanePeaks[t.key] ?? null, volume, muted: volume === 0 };
        })
      : [],
  );

  /** A lane's slider moved: a level set by hand replaces the one a mute remembered. */
  function onvolume(key: string, volume: number) {
    if (editLocked) return;
    delete unmuteLevels[key];
    setVolume(key, volume);
  }

  function onmute(key: string) {
    if (editLocked) return;
    const next = toggleMute(volumeOf(key), unmuteLevels[key] ?? null);
    if (next.remembered === null) delete unmuteLevels[key];
    else unmuteLevels[key] = next.remembered;
    setVolume(key, next.volume);
  }

  async function loadAudio(d: EditorData) {
    const tracks = audioTracks(d);
    let failed: string[];
    try {
      failed = await engine.load(tracks, (loaded, total) => {
        if (!closed) audioLoad = { loaded, total };
      });
    } catch {
      // No sound at all (e.g. Web Audio isn't available): the video still plays, silently.
      if (closed) return;
      silent = true;
      toast("Couldn't play sound for this clip.", "error");
      audioReady = true;
      return;
    }
    if (closed) return;
    for (const t of tracks) {
      if (failed.includes(t.key)) toast(`Couldn't load the ${t.name} track`, "error");
    }
    audioReady = true;
  }

  onMount(() => {
    // Listening before `load` starts, so its first steps aren't missed.
    const stopPreparing = on("editor-progress", ({ id, ...progress }) => {
      if (id === clip.id && !data) preparing = progress;
    });
    void load();

    // The audio clock moves the playhead; the video follows it.
    let frame = requestAnimationFrame(function tick() {
      if (playing) follow();
      frame = requestAnimationFrame(tick);
    });

    const stopProgress = on("export-progress", ({ id, progress }) => {
      if (id === clip.id && exportProgress !== null) exportProgress = progress;
    });
    const stopFilmstrip = on("filmstrip-progress", ({ id, frames }) => {
      if (id === clip.id) stripFrames = frames;
    });

    return () => {
      closed = true;
      // Leaving with a change not saved yet (or whose save failed): save it now.
      if (autosave.pending && latest?.saveable && !deleted) {
        api.saveEdit(clip.id, latest.edit, latest.thumb).catch(leaveSaveFailed);
      }
      autosave.stop();
      cancelAnimationFrame(frame);
      stopProgress();
      stopFilmstrip();
      stopPreparing();
      clearTimeout(settleTimer);
      clearTimeout(startTimer);
      video?.pause();
      engine.close();
      api.closeEditor(clip.id).catch(() => {});
    };
  });

  // Playback gain: the sliders' 0–100% is a gain of 0.0–1.0; Master scales every track. A track
  // still loading drops its gain, so these run again once the audio is ready.
  $effect(() => {
    if (audioReady && !silent) engine.setMaster(masterVolume / 100);
  });
  $effect(() => {
    if (audioReady && !silent) engine.setGain("game", gameVolume / 100);
  });
  $effect(() => {
    if (audioReady && !silent) engine.setGain("mic", micVolume / 100);
  });
  $effect(() => {
    if (audioReady && !silent) engine.setGain("voice", voiceVolume / 100);
  });
  $effect(() => {
    if (!audioReady || silent) return;
    for (const t of data?.extraTracks ?? []) engine.setGain(`track-${t.index}`, (extraVolumes[t.index] ?? 100) / 100);
  });

  // Trimming or splitting while playing: play the new parts from where the playhead is.
  let playingRanges: TrimRange[] = [];
  $effect(() => {
    const ranges = playRanges;
    untrack(() => {
      if (playing && !sameTrims(ranges, playingRanges)) startPlayback(position);
    });
  });

  function sameTrims(a: TrimRange[], b: TrimRange[]): boolean {
    return a.length === b.length && a.every((r, i) => r.start === b[i].start && r.end === b[i].end);
  }

  // While dragging, only one seek runs at a time; the newest target waits in `pendingSeek` and
  // is applied when the current seek lands. Each seek first freezes the last frame on a canvas
  // overlay, so WebKitGTK's black frame during decoding never shows.
  let freeze = $state<HTMLCanvasElement>();
  let frozen = $state(false);
  let pendingSeek: number | null = null;
  /** How long a video seek takes here (a moving average): seeks while playing aim that far ahead. */
  let seekAverageMs = SEEK_LATENCY_START_MS;
  /** When the seek in flight was asked for (`performance.now()`); null when none is. */
  let seekStartedAt: number | null = null;

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
      if (!video?.seeking) {
        frozen = false;
        // The video caught up with the filmstrip picture: show the video again.
        if (!scrubAhead) scrubFrame = null;
      }
    };
    const v = video as (HTMLVideoElement & { requestVideoFrameCallback?: (cb: () => void) => number }) | undefined;
    v?.requestVideoFrameCallback?.(() => requestAnimationFrame(done));
    unfreezeTimer = setTimeout(done, 250);
  }

  /** Moves the video's picture (not the audio). */
  function seekVideo(ms: number) {
    if (!video) return;
    if (video.seeking) {
      pendingSeek = ms;
      return;
    }
    freezeFrame();
    seekStartedAt = performance.now();
    video.currentTime = ms / 1000;
  }

  /** Moves the playhead: while playing, the audio plays on from there. */
  function seek(ms: number) {
    position = ms;
    if (playing) startPlayback(ms);
    else if (scrubbing) scrubTo(ms);
    else seekVideo(ms);
  }

  /**
   * While scrubbing: shows the filmstrip picture at `ms` at once, and seeks the video there only
   * once the pointer rests. Without a picture (not made yet, or deleted), seeks the video.
   */
  function scrubTo(ms: number) {
    const index = data && !mediaError ? previewIndex(ms, stripFrames) : null;
    if (index === null || !data) {
      // The picture stays up until the video has painted this frame.
      scrubAhead = false;
      clearTimeout(settleTimer);
      seekVideo(ms);
      return;
    }
    scrubFrame = frameUrl(data.filmstrip.dir, index);
    scrubAhead = true;
    clearTimeout(settleTimer);
    settleTimer = setTimeout(settleScrub, SCRUB_SETTLE_MS);
  }

  /** Seeks the video to the playhead's exact frame; the picture hides once that frame is painted. */
  function settleScrub() {
    clearTimeout(settleTimer);
    if (!scrubAhead) return;
    scrubAhead = false;
    seekVideo(position);
  }

  /** The pointer went down on the timeline: pause (even if it never moves). */
  function onscrubstart() {
    gestures.start("scrub");
    scrubbing = true;
    wasPlaying = playing;
    if (playing) pause();
  }

  function onscrubend() {
    gestures.end("scrub");
    scrubbing = false;
    settleScrub();
    if (wasPlaying && !playing && !mediaError && audioReady) startPlayback(position);
    wasPlaying = false;
  }

  /** A filmstrip picture failed to load: the strip is gone, so scrub with the video from now on. */
  function onscruberror() {
    stripFrames = 0;
    scrubFrame = null;
    if (scrubAhead) {
      scrubAhead = false;
      clearTimeout(settleTimer);
      seekVideo(position);
    }
  }

  /** The segment the audio played on the last frame: a new one that doesn't follow it is a cut. */
  let lastSegment: Segment | null = null;
  /** The segment whose cut the video already seeked across (ahead of the audio getting there). */
  let seekedPastCut: Segment | null = null;

  /** Starts the audio at `ms` (or the next kept part) and the video with it. */
  function startPlayback(ms: number) {
    if (!video) return;
    if (silent) {
      startSilentPlayback(ms);
      return;
    }
    // The picture starts first; the sound joins as soon as the video's position moves (`follow`
    // → `beginAudio`), from the video's own position. Started together, nothing has to catch up:
    // on WebKitGTK the video takes a while to get going, and audio running ahead meanwhile set off
    // correction seeks that each flashed black. Its `playing` event isn't waited for: WebKitGTK
    // sometimes never sends it (play pressed right after a scrub), which left the sound silent.
    playingRanges = playRanges;
    lastSegment = null;
    seekedPastCut = null;
    playing = true;
    const from = schedule(ms, playRanges)[0]?.from ?? ms;
    position = from;
    const seekNeeded = Math.abs(video.currentTime * 1000 - from) > 80;
    // Moving past where the video is now (or lands after the seek) means it really plays.
    startVideoMs = seekNeeded ? from : Math.max(from, video.currentTime * 1000);
    if (seekNeeded) seekVideo(from);
    video.playbackRate = 1;
    starting = true;
    clearTimeout(startTimer);
    startTimer = setTimeout(beginAudio, START_TIMEOUT_MS);
    video.play().catch(() => {});
    showVideo();
  }

  /** Playback was asked for and the video is getting going; the sound hasn't started yet. */
  let starting = false;
  let startTimer: ReturnType<typeof setTimeout> | undefined;
  /** Where the video was asked to play from; once its position moves past it, the sound starts. */
  let startVideoMs = 0;
  /** The sound starts anyway if the video hasn't moved by then (a stuck video). */
  const START_TIMEOUT_MS = 700;

  /** The video plays: the sound starts from where the picture is. */
  function beginAudio() {
    if (!starting || !video) return;
    starting = false;
    clearTimeout(startTimer);
    engine.play(audioStartMs(video.currentTime * 1000), playingRanges);
    lastSegment = engine.segment();
    if (lastSegment) position = lastSegment.from;
  }

  /** Hides a scrub picture still over the video, unless a seek is still bringing the video there. */
  function showVideo() {
    if (!video?.seeking && pendingSeek === null) scrubFrame = null;
  }

  /** Without sound: the video plays from `ms` (or the next kept part) and `followVideo` leads. */
  function startSilentPlayback(ms: number) {
    if (!video) return;
    playingRanges = playRanges;
    seekedPastCut = null;
    playing = true;
    position = playFrom(ms, playRanges);
    if (videoCorrection(video.currentTime * 1000, position, seekAverageMs, Infinity)) seekVideo(position);
    video.playbackRate = 1;
    video.play().catch(() => {});
    showVideo();
  }

  /** Each frame while playing without sound: the playhead follows the video, which skips cuts and loops. */
  function followVideo() {
    if (!video || video.seeking || pendingSeek !== null) return;
    const ms = video.currentTime * 1000;
    position = ms;
    // The video can end a little before the clip's recorded length: loop from there too.
    const to = video.ended ? playRanges[0].start : videoLedJump(ms, playRanges);
    if (to === null) return;
    seekVideo(to);
    // At the video's end it stopped by itself.
    if (video.paused) video.play().catch(() => {});
  }

  function pause() {
    const wasStarting = starting;
    starting = false;
    clearTimeout(startTimer);
    position = wasStarting ? position : (engine.positionMs() ?? position);
    engine.stop();
    playing = false;
    lastSegment = null;
    seekedPastCut = null;
    if (video) {
      video.pause();
      video.playbackRate = 1;
      // The video trails or leads the audio a little: the playhead takes the frame on screen, so a
      // split lands where the user sees it. No seek: on WebKitGTK a seek right after pausing
      // flashes a black frame.
      if (!mediaError && !video.seeking) position = video.currentTime * 1000;
    }
  }

  /** Each frame while playing: the playhead follows the audio, and the video follows both. */
  function follow() {
    if (!video) return;
    if (silent) {
      followVideo();
      return;
    }
    // Waiting for the video to get going: the playhead shows the picture's time meanwhile.
    if (starting) {
      if (!video.seeking) position = video.currentTime * 1000;
      if (videoStarted(startVideoMs, video.currentTime * 1000, video.seeking)) beginAudio();
      return;
    }
    const seg = engine.segment();
    const ms = engine.positionMs();
    if (!seg || ms === null) {
      // The audio played the last kept part: loop from the first.
      startPlayback(playRanges[0].start);
      return;
    }
    position = ms;
    const cut = lastSegment !== null && seg !== lastSegment && seg.from !== lastSegment.to;
    const seekedAhead = cut && seekedPastCut === lastSegment;
    lastSegment = seg;
    if (cut) {
      seekedPastCut = null;
      // Already on its way there from before the cut: no second seek.
      if (seekedAhead) return;
      video.playbackRate = 1;
      seekVideo(Math.min(ms + seekAverageMs, seg.to));
      return;
    }
    // The video already moved to the next part; the audio gets there in a moment.
    if (seekedPastCut === seg) return;
    const ahead = cutSeekAhead(ms, seg, engine.nextSegment(), seekAverageMs);
    if (ahead !== null) {
      seekedPastCut = seg;
      video.playbackRate = 1;
      seekVideo(ahead);
      return;
    }
    // One drift seek at a time: the next is decided once this one landed.
    if (video.seeking || pendingSeek !== null) return;
    const correction = videoCorrection(video.currentTime * 1000, ms, seekAverageMs, seg.to);
    if (!correction) {
      if (video.playbackRate !== 1) video.playbackRate = 1;
    } else if ("rate" in correction) {
      if (video.playbackRate !== correction.rate) video.playbackRate = correction.rate;
    } else {
      video.playbackRate = 1;
      seekVideo(correction.seekMs);
    }
  }

  function togglePlay() {
    if (!video) return;
    // Pausing works even once the video failed, so the sound can always be stopped.
    if (playing) {
      pause();
      return;
    }
    if (mediaError || !audioReady) return;
    // At the end of the last part (or within a frame or so of it): start over from the first.
    const last = playRanges[playRanges.length - 1];
    startPlayback(position >= last.end - 50 ? playRanges[0].start : position);
  }

  /** The video can't be played: stop the sound too, or it would play on with no way to pause. */
  function onmediaerror() {
    mediaError = true;
    if (playing) pause();
  }

  function onseeked() {
    if (seekStartedAt !== null) {
      seekAverageMs = seekLatency(seekAverageMs, performance.now() - seekStartedAt);
      seekStartedAt = null;
    }
    if (pendingSeek !== null && video) {
      const next = pendingSeek;
      pendingSeek = null;
      seekStartedAt = performance.now();
      video.currentTime = next / 1000;
      return;
    }
    // Unfreeze only once the new frame is actually on screen (WebKitGTK paints it after
    // `seeked`); fall back to a short delay where requestVideoFrameCallback is missing.
    unfreezeAfterPaint();
  }

  function onloadedmetadata() {
    if (!video) return;
    // A (re)loaded video shows its own picture, not one left from a scrub.
    scrubFrame = null;
    videoSize = { width: video.videoWidth, height: video.videoHeight };
    // The engine plays the sound: the video's own audio stays off (and muted) for good.
    const tracks = (video as HTMLVideoElement & { audioTracks?: Iterable<{ enabled: boolean }> }).audioTracks;
    if (tracks) for (const t of tracks) t.enabled = false;
  }

  function onhandlemove(_handle: "start" | "end" | "thumb" | "split", ms: number) {
    // Show the frame under the handle while paused; while playing, the audio clock moves it.
    if (!playing) seek(ms);
  }

  function splitHere() {
    // The clip can't change while it uploads or exports.
    if (editLocked) return;
    const next = splitAt(pieces, position, trim);
    if (next) pieces = next;
    else toast("Can't split here: too close to a split or the trim, or outside the trim", "error");
  }

  /** Adds a zoom at the playhead and picks it, so its box can be placed on the video. */
  function zoomHere() {
    if (editLocked) return;
    const next = addZoomAt(zooms, position, trim);
    if (!next) {
      toast("Can't zoom here: inside another zoom, too close to the next one, or outside the trim", "error");
      return;
    }
    zooms = next;
    selectedZoom = zoomIndexAt(next, Math.round(position));
    selectedSubtitle = null;
  }

  /** Adds a subtitle at the playhead and opens its text box. */
  function subtitleHere() {
    if (editLocked) return;
    const next = addSubtitleAt(subtitles, position, trim);
    if (!next) {
      toast("Can't add a subtitle here: inside another one, too close to the next one, or outside the trim", "error");
      return;
    }
    if (playing) pause();
    history?.record(editState);
    subtitles = next;
    const i = subtitleIndexAt(next, Math.round(position));
    selectedSubtitle = i;
    selectedZoom = null;
    editingSubtitle = i;
  }

  function removeSubtitleAt(i: number) {
    if (editLocked) return;
    subtitles = removeSubtitle(subtitles, i);
    selectedSubtitle = null;
    editingSubtitle = null;
  }

  function removeZoomAt(i: number) {
    if (editLocked) return;
    zooms = removeZoom(zooms, i);
    selectedZoom = null;
  }

  /** The picked zoom's box was moved or resized on the video. */
  function onzoombox(box: { zoom: number; x: number; y: number }) {
    if (editLocked || selectedZoom === null) return;
    zooms = setZoomBox(zooms, selectedZoom, box);
  }

  /** A zoom's drag (on the lane or the video) is one undo step. */
  function onzoomgesture(active: boolean) {
    if (active) gestures.start("zoom");
    else gestures.end("zoom");
  }

  /** One frame, in ms. */
  const frameMs = $derived(stepMs(data?.fps ?? 0));

  /** Moves the playhead by `ms` (clamped to the clip); playback goes on from there. Not an edit. */
  function stepBy(ms: number) {
    if (!data) return;
    seek(Math.min(duration, Math.max(0, position + ms)));
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    // Text fields and dialogs keep their keys, and a slider focused from the keyboard its arrows;
    // Space plays even on a focused button (its press is prevented, so it isn't clicked).
    const action = editorKeyAction(e, keyTarget(e.target instanceof Element ? e.target : null), uploadOpen);
    if (!action) return;
    e.preventDefault();
    if (action.kind === "undo") undo();
    else if (action.kind === "redo") redo();
    else if (action.kind === "step") stepBy(action.direction * (action.second ? 1000 : frameMs));
    else if (action.kind === "split") splitHere();
    else if (action.kind === "zoom") zoomHere();
    else if (action.kind === "subtitle") subtitleHere();
    else if (action.kind === "delete") {
      if (selectedZoom !== null) removeZoomAt(selectedZoom);
      else if (selectedSubtitle !== null) removeSubtitleAt(selectedSubtitle);
    }
    else togglePlay();
  }

  /**
   * The pointer let go: a timeline control it clicked or dragged gives the keyboard back, so Space
   * and the arrows reach the editor again (one focused with Tab keeps its own keys).
   */
  function onpointerup() {
    gestures.end("slider");
    const focused = document.activeElement;
    if (focused instanceof HTMLElement && focused.closest(".timeline") && focused.matches(RELEASES_FOCUS)) focused.blur();
  }

  /** Writes the edit as it is now; throws when the save is refused (e.g. while uploading). */
  async function writeEdit() {
    const next = edit;
    const nextThumb = savedThumb;
    await api.saveEdit(clip.id, next, nextThumb);
    stored = next;
    storedThumb = nextThumb;
    if (data) {
      data.clip = {
        ...data.clip,
        thumbnailMs: nextThumb,
        trimStartMs: next.trimStartMs,
        trimEndMs: next.trimEndMs,
        gameVolume: next.gameVolume,
        micVolume: next.micVolume,
        voiceVolume: next.voiceVolume,
        masterVolume: next.masterVolume,
        extraVolumes: next.extraVolumes,
        blurVideo: next.blurVideo,
        removedRanges: next.removedRanges,
        zoomRanges: next.zoomRanges,
        subtitles: next.subtitles,
        uploadId: null,
        uploadOffset: null,
      };
    }
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

  function setGame() {
    tagging = { value: gameName ?? "", games: [] };
    api.listGames().then((games) => tagging && (tagging.games = games)).catch(() => {});
  }

  async function submitGame(e: SubmitEvent) {
    e.preventDefault();
    if (!tagging) return;
    const name = tagging.value.trim() || null;
    try {
      await api.setClipGame(clip.id, name);
      gameName = name;
      tagging = null;
    } catch (err) {
      toast(errorMessage(err), "error");
    }
  }

  /** The clip is uploaded: into the feed or link-only, in place (same upload and link). */
  async function toggleVisibility() {
    moreOpen = false;
    const id = data?.clip.serverClipId;
    if (!id || !data) return;
    const visibility = data.clip.visibility === "link" ? "feed" : "link";
    try {
      await api.updateUpload(id, { visibility });
      if (data) data.clip = { ...data.clip, visibility };
      toast(visibility === "feed" ? "Now in the feed." : "Now link-only.", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  function showInFolder() {
    moreOpen = false;
    api.showInFolder(clip.id).catch((e) => toast(errorMessage(e), "error"));
  }

  /** A volume slider is grabbed: its drag is one undo step. */
  function onpointerdown(e: PointerEvent) {
    if ((e.target as Element).closest?.(".editor input[type='range']")) gestures.start("slider");
  }

  function closeMore(e: MouseEvent) {
    if (moreOpen && !(e.target as Element).closest?.(".more")) moreOpen = false;
  }

  async function askDelete() {
    moreOpen = false;
    pause();
    // Uploaded from this editor? The library knows its server copy now; `clip` may not.
    let current = data?.clip ?? clip;
    try {
      current = (await api.listClips({ game: null, sort: "newestFirst" })).find((c) => c.id === clip.id) ?? current;
    } catch {
      // Use what we have.
    }
    removing = current;
  }

  async function confirmDelete(where: RemoveWhere) {
    if (!removing) return;
    const target = removing;
    deleting = true;
    try {
      // The server first: if it refuses, the clip is still whole on this PC.
      if (where === "everywhere" && target.serverClipId) await api.deleteUpload(target.serverClipId);
      await api.deleteClip(target.id, where !== "library" && !target.fileMissing);
      toast(
        where === "everywhere" ? "Deleted from this PC and the server." : where === "local" ? "Clip deleted." : "Removed from library.",
        "success",
      );
      removing = null;
      deleted = true;
      ondeleted();
    } catch (err) {
      toast(errorMessage(err), "error");
    } finally {
      deleting = false;
    }
  }

  async function onupload() {
    // The upload exports the saved edit, so save first: it uploads the cut the user sees.
    if (await autosave.flush()) {
      pause();
      uploadOpen = true;
    }
  }
</script>

<svelte:window
  {onkeydown}
  onclick={closeMore}
  {onpointerdown}
  {onpointerup}
  onpointercancel={() => gestures.end("slider")}
/>

<section class="editor">
  <header class="bar">
    <button class="btn btn-ghost back" onclick={onclose}>
      <Icon name="back" />
      Clips
    </button>
    <div class="title">
      <h1 title={data?.clip.title ?? clip.title}>{data?.clip.title ?? clip.title}</h1>
      <button class="game" class:unknown={!gameName} onclick={setGame} title="Set game">
        {#if gameName}<span class="game-dot" style:--hue={hueOf(gameName)}></span>{/if}
        {gameName ?? "Unknown game"}
        <Icon name="chevronDown" size={13} />
      </button>
    </div>
    <div class="actions">
      {#if data}
        {#if !audioReady}
          <span class="save-status" role="status" title="Play starts once the audio is loaded">
            <span class="spinner"></span>
            {#if audioLoad?.total && audioLoad.loaded < audioLoad.total}
              Loading audio… {Math.round((audioLoad.loaded / audioLoad.total) * 100)}% · {formatSize(audioLoad.total)}
            {:else if audioLoad?.total}
              Decoding audio…
            {:else}
              Loading audio…
            {/if}
          </span>
        {/if}
        {#if changed && !kept.length}
          <span class="save-status warn" role="status">Not saved: {nothingKept.toLowerCase()}</span>
        {:else if editLocked || saveStatus === "waiting"}
          <span class="save-status" role="status">{exporting ? "Exporting" : "Uploading"} — you can edit when it's done</span>
        {:else if saveStatus === "saving"}
          <span class="save-status" role="status"><span class="spinner"></span> Saving…</span>
        {:else if saveStatus === "saved"}
          <span class="save-status" role="status">✓ Saved</span>
        {:else}
          <span class="save-status failed" role="status">
            <Icon name="alert" size={15} /> Couldn't save
            <button class="btn btn-sm" onclick={() => autosave.flush()}><Icon name="retry" size={14} /> Retry</button>
          </span>
        {/if}
        <button class="btn" onclick={onexport} disabled={busy || !kept.length} title={kept.length ? "Export a copy of the edited clip" : nothingKept}>
          {#if exporting}
            <span class="spinner"></span> Exporting… {Math.round((exportProgress ?? 0) * 100)}%
          {:else}
            <Icon name="film" size={16} /> Export…
          {/if}
        </button>
        <button class="btn btn-primary" onclick={onupload} disabled={busy || !kept.length} title={kept.length ? undefined : nothingKept}>
          {#if uploading}
            <span class="spinner"></span> Uploading…
          {:else}
            <Icon name="upload" size={16} /> Upload
          {/if}
        </button>
      {/if}
      <div class="more">
        <button
          class="btn btn-ghost btn-icon"
          onclick={() => (moreOpen = !moreOpen)}
          aria-expanded={moreOpen}
          aria-haspopup="menu"
          aria-label="More actions"
          title="More actions"
        >
          <Icon name="more" size={18} />
        </button>
        {#if moreOpen}
          <div class="menu" role="menu" use:menu={{ close: () => (moreOpen = false) }}>
            {#if data?.clip.serverClipId}
              <button role="menuitem" onclick={toggleVisibility} disabled={uploading}>
                <Icon name="eye" size={16} />
                {data.clip.visibility === "link" ? "Show in feed" : "Hide from feed"}
              </button>
            {/if}
            <button role="menuitem" onclick={showInFolder}><Icon name="folder" size={16} /> Show in folder</button>
            <button
              role="menuitem"
              class="danger"
              onclick={askDelete}
              disabled={uploading || deleting}
              title={uploading ? "Wait for the upload to finish" : undefined}
            >
              <Icon name="trash" size={16} /> Delete
            </button>
          </div>
        {/if}
      </div>
    </div>
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
    <div class="center" role="status">
      <p class="prep-title">Preparing the clip</p>
      <div class="prep">
        <div class="progress" class:indeterminate={prepStep.fraction === null} style:--value={prepStep.fraction ?? undefined}>
          <div></div>
        </div>
        <p class="prep-step">
          <span>{prepStep.label}</span>
          {#if prepStep.fraction !== null}<span class="prep-pct">{Math.round(prepStep.fraction * 100)}%</span>{/if}
        </p>
      </div>
    </div>
  {:else}
    <div class="stage" bind:clientWidth={stageWidth} bind:clientHeight={stageHeight}>
      <!-- The picture's own area (no letterbox), so a zoom scales around a point of the picture
           and never spills into the black bars, as in the export. -->
      <div
        class="frame"
        style:left="{videoRect.left}px"
        style:top="{videoRect.top}px"
        style:width="{videoRect.width}px"
        style:height="{videoRect.height}px"
        style:--zoom={view.scale}
        style:--zoom-origin="{view.x * 100}% {view.y * 100}%"
      >
        <!-- svelte-ignore a11y_media_has_caption -->
        <video
          bind:this={video}
          src={data.videoUrl}
          crossorigin="anonymous"
          muted
          playsinline
          preload="auto"
          onseeked={onseeked}
          onerror={onmediaerror}
          onclick={togglePlay}
          {onloadedmetadata}
          style:filter={previewBlur}
        ></video>
        <canvas bind:this={freeze} class="freeze" class:show={frozen} aria-hidden="true" style:filter={previewBlur}></canvas>
        {#if scrubFrame}
          <img class="scrub" src={scrubFrame} alt="" aria-hidden="true" style:filter={previewBlur} onerror={onscruberror} />
        {/if}
        <!-- Drawn like the export (src-tauri/src/subtitles.rs): sized from the picture's height,
             never zoomed or blurred. -->
        {#if shownSubtitle}
          <p class="subtitle" style:--h="{videoRect.height}px" style:--w="{videoRect.width}px">{shownSubtitle.text}</p>
        {/if}
      </div>
      {#if placingZoom && selectedZoom !== null && zooms[selectedZoom] && !editLocked && !mediaError}
        <ZoomBox zoom={zooms[selectedZoom]} rect={videoRect} onchange={onzoombox} ongesture={onzoomgesture} />
      {/if}
      {#if mediaError}
        <div class="media-error">
          <Icon name="alert" size={28} />
          <p>Klipt can't play this clip here. You can still trim, export and upload it.</p>
        </div>
      {/if}
    </div>

    <div class="timeline">
      <div class="transport">
        <span class="transport-side"></span>
        <div class="transport-main">
        <button class="btn btn-ghost btn-icon" onclick={() => stepBy(-frameMs)} aria-label="Previous frame" title="Previous frame (←)">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M6 5h2v14H6zM18.2 5.6v12.8a.7.7 0 0 1-1.1.6L9.4 12.6a.7.7 0 0 1 0-1.2l7.7-6.4a.7.7 0 0 1 1.1.6z" />
          </svg>
        </button>
        <button
          class="btn btn-primary btn-icon play"
          onclick={togglePlay}
          disabled={(mediaError || !audioReady) && !playing}
          aria-label={playing ? "Pause" : "Play"}
          title={!audioReady ? "Loading audio…" : playing ? "Pause (Space)" : "Play (Space)"}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            {#if playing}
              <path d="M7 5h3.5v14H7zM13.5 5H17v14h-3.5z" />
            {:else}
              <path d="M8 5.5v13a.8.8 0 0 0 1.2.7l10.4-6.5a.8.8 0 0 0 0-1.4L9.2 4.8A.8.8 0 0 0 8 5.5z" />
            {/if}
          </svg>
        </button>
        <button class="btn btn-ghost btn-icon" onclick={() => stepBy(frameMs)} aria-label="Next frame" title="Next frame (→)">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M16 5h2v14h-2zM5.8 5.6v12.8a.7.7 0 0 0 1.1.6l7.7-6.4a.7.7 0 0 0 0-1.2L6.9 5a.7.7 0 0 0-1.1.6z" />
          </svg>
        </button>
        <span class="clock"><strong>{formatTenths(position)}</strong> / {formatClock(duration)}</span>
        <span class="kept">Kept <strong>{formatClock(keptLength(pieces, trim))}</strong></span>
        <button
          class="btn btn-sm split-btn"
          onclick={splitHere}
          disabled={mediaError || editLocked}
          title="Split at the playhead (S), then switch the parts you don't want off"
        >
          <Icon name="scissors" size={15} /> Split
        </button>
        <button
          class="btn btn-sm"
          onclick={zoomHere}
          disabled={mediaError || editLocked}
          title="Zoom in at the playhead (Z), then drag the box on the video"
        >
          <Icon name="zoom" size={15} /> Zoom
        </button>
        <button
          class="btn btn-sm"
          onclick={subtitleHere}
          disabled={mediaError || editLocked}
          title="Add a subtitle at the playhead (T)"
        >
          <Icon name="subtitles" size={15} /> Subtitle
        </button>
        </div>
        <label class="master" title="The whole clip: every track together">
          <Icon name="volume" size={17} />
          <input
            class="range"
            type="range"
            min="0"
            max="100"
            step="1"
            value={masterVolume}
            disabled={editLocked}
            style:--fill="{masterVolume}%"
            aria-label="Master volume"
            oninput={(e) => {
              if (!editLocked) masterVolume = Number(e.currentTarget.value);
            }}
          />
          <output>{masterVolume === 0 ? "Off" : `${masterVolume}%`}</output>
        </label>
      </div>
      <TrackLanes
        {duration}
        {trim}
        {pieces}
        {position}
        tracks={laneTracks}
        blur={blurVideo}
        locked={editLocked}
        {onvolume}
        {onmute}
        onblur={(b) => {
          if (!editLocked) blurVideo = b;
        }}
        onseek={seek}
        {onscrubstart}
        {onscrubend}
        {zooms}
        {selectedZoom}
        onzoomselect={(i) => (selectedZoom = i)}
        onzoomchange={(next) => {
          if (!editLocked) zooms = next;
        }}
        {onzoomgesture}
        onzoomremove={removeZoomAt}
      >
        {#snippet video()}
          <TrimBar
            {duration}
            bind:start={trim.start}
            bind:end={trim.end}
            bind:thumb
            bind:pieces
            {position}
            strip={data ? { dir: data.filmstrip.dir, ready: stripFrames } : null}
            onseek={seek}
            {onhandlemove}
            {onscrubstart}
            {onscrubend}
            locked={editLocked}
          />
        {/snippet}
      </TrackLanes>
      <p class="hints">
        <span><kbd>Space</kbd> play</span>
        <span><kbd>S</kbd> split</span>
        <span><kbd>Z</kbd> zoom</span>
        <span><kbd>T</kbd> subtitle</span>
        <span><kbd>←</kbd><kbd>→</kbd> one frame</span>
        <span><kbd>Shift</kbd>+<kbd>←</kbd><kbd>→</kbd> one second</span>
        <span><kbd>Ctrl</kbd>+<kbd>Z</kbd> undo</span>
      </p>
    </div>
  {/if}
</section>

{#if uploadOpen && data}
  <UploadDialog clip={data.clip} onclose={() => (uploadOpen = false)} />
{/if}

{#if tagging}
  <Modal title="Set game" onclose={() => (tagging = null)}>
    <form id="editor-game-form" onsubmit={submitGame}>
      <label class="field">
        <span>Game</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={tagging.value} list="editor-known-games" placeholder="Unknown game" autofocus />
        <datalist id="editor-known-games">
          {#each tagging.games as g (g.name)}<option value={g.name}></option>{/each}
        </datalist>
      </label>
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (tagging = null)}>Cancel</button>
      <button class="btn btn-primary" type="submit" form="editor-game-form">Save</button>
    {/snippet}
  </Modal>
{/if}

{#if removing}
  <DeleteClipDialog
    local={[{ title: removing.title, fileMissing: removing.fileMissing, uploaded: !!removing.serverClipId }]}
    busy={deleting}
    onconfirm={confirmDelete}
    onclose={() => (removing = null)}
  />
{/if}

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    height: 100%;
    padding: 1.1rem 2rem 1.5rem;
    /* The editor is a tool, not a page: dragging across it must not
       highlight labels and rows. Text fields keep their selection. */
    -webkit-user-select: none;
    user-select: none;
  }

  .editor :global(:is(input, textarea)) {
    -webkit-user-select: text;
    user-select: text;
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
    gap: 0.85rem;
    min-width: 0;
  }

  h1 {
    overflow: hidden;
    font-size: 1.45rem;
    font-weight: 750;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .game {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 0.45rem;
    padding: 0.2rem 0.45rem 0.2rem 0.55rem;
    border: 1px solid transparent;
    border-radius: 999px;
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-size: 0.9rem;
    cursor: pointer;
    transition:
      border-color 0.15s,
      color 0.15s;
  }

  .game:hover,
  .game:focus-visible {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .unknown {
    font-style: italic;
  }

  .actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 0.5rem;
    margin-left: auto;
  }

  .save-status {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    margin-right: 0.35rem;
    color: var(--text-muted);
    font-size: 0.85rem;
    white-space: nowrap;
  }

  .save-status .spinner {
    width: 14px;
    height: 14px;
  }

  .save-status.warn {
    color: var(--warning);
  }

  .save-status.failed {
    color: var(--danger-text, var(--danger));
  }

  .more {
    position: relative;
  }

  .menu {
    position: absolute;
    top: calc(100% + 0.4rem);
    right: 0;
    z-index: 30;
    display: flex;
    flex-direction: column;
    min-width: 12rem;
    padding: 0.35rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-2);
    box-shadow: var(--shadow);
    animation: menu-in 0.15s var(--ease-out);
  }

  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.5rem 0.6rem;
    border: none;
    border-radius: 8px;
    background: none;
    color: var(--text-muted);
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text);
  }

  .menu button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .menu .danger {
    color: var(--danger-text, var(--danger));
  }

  .menu .danger:hover:not(:disabled) {
    background: var(--danger-soft);
    color: var(--danger-text, var(--danger));
  }

  .prep-title {
    color: var(--text);
    font-weight: 650;
  }

  .prep {
    width: min(320px, 70%);
  }

  .prep-step {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    margin-top: 0.55rem;
    font-size: 0.85rem;
  }

  .prep-pct {
    font-variant-numeric: tabular-nums;
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
    border-radius: var(--radius-lg);
    background: #000;
    box-shadow: 0 24px 60px -30px rgba(124, 92, 255, 0.35);
  }

  .frame {
    position: absolute;
    overflow: hidden;
  }
  .frame > :is(video, .freeze, .scrub) {
    transform: scale(var(--zoom, 1));
    transform-origin: var(--zoom-origin, 50% 50%);
  }
  .freeze,
  .scrub {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
  }
  .freeze {
    visibility: hidden;
  }
  /* The export's fixed style: 5.5% of the height, outline 0.35% (at least 2 px), 6% from the
     bottom, 5% side margins. The stroke is centred on the outline, so it is drawn twice as wide. */
  .subtitle {
    position: absolute;
    right: calc(var(--w) * 0.05);
    bottom: calc(var(--h) * 0.06);
    left: calc(var(--w) * 0.05);
    margin: 0;
    color: #fff;
    font-family: "Klipt Subtitles", sans-serif;
    font-size: calc(var(--h) * 0.055);
    font-weight: 700;
    line-height: 1.2;
    text-align: center;
    overflow-wrap: anywhere;
    -webkit-text-stroke: calc(max(2px, var(--h) * 0.0035) * 2) #000;
    paint-order: stroke fill;
    pointer-events: none;
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
    flex-direction: column;
    gap: 0.6rem;
    flex: none;
  }

  /* Controls centred under the video; the master volume at the right. */
  .transport {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 0.9rem;
  }

  .transport-main {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem 0.9rem;
    flex-wrap: wrap;
  }

  .master {
    justify-self: end;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    color: var(--text-muted);
  }

  .master .range {
    width: 8rem;
  }

  .master output {
    min-width: 2.6rem;
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
  }

  .play {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 50%;
  }

  .clock,
  .kept {
    color: var(--text-muted);
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .clock strong,
  .kept strong {
    color: var(--text);
    font-weight: 650;
  }

  .hints {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.3rem 1.1rem;
    color: var(--text-faint);
    font-size: 0.78rem;
  }

  kbd {
    display: inline-block;
    min-width: 1.4rem;
    margin-right: 0.2rem;
    padding: 0.05rem 0.35rem;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--text-muted);
    font: inherit;
    font-size: 0.74rem;
    text-align: center;
  }
</style>
