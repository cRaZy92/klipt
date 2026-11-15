<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api, errorMessage, on, type ClipView, type EditInput, type EditorData, type GameCount } from "../lib/api";
  import type { RemoveWhere } from "../lib/removal";
  import { AudioEngine } from "../lib/audioEngine";
  import {
    cutSeekAhead,
    SEEK_LATENCY_START_MS,
    seekLatency,
    videoCorrection,
    videoLedJump,
    type Segment,
  } from "../lib/playSchedule";
  import { exportFileName, hueOf } from "../lib/format";
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
  import { toast } from "../lib/toasts.svelte";
  import { isBusy, uploads } from "../lib/uploads.svelte";
  import DeleteClipDialog from "../components/DeleteClipDialog.svelte";
  import Icon from "../components/Icon.svelte";
  import Modal from "../components/Modal.svelte";
  import TrimBar from "../components/TrimBar.svelte";
  import UploadDialog from "../components/UploadDialog.svelte";

  let {
    clip,
    onclose,
    ondeleted,
    dirty = $bindable(false),
  }: {
    clip: ClipView;
    /** Back to the library (the app asks about unsaved changes). */
    onclose: () => void;
    /** The clip was deleted: back to the library without asking about unsaved changes. */
    ondeleted: () => void;
    /** Whether there are unsaved changes, for the app's "Discard changes?" guard. */
    dirty?: boolean;
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
  /** "Blur video": uploads and exports blur the picture (a clip where only the sound matters). */
  let blurVideo = $state(false);
  /** The thumbnail's frame; null = automatic. */
  let thumb = $state<number | null>(null);
  /** The edit as last loaded or saved; `dirty` compares against it. */
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
  let playing = $state(false);
  let position = $state(0);

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

  let saving = $state(false);
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
  const busy = $derived(saving || exporting || uploading);
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
    };
  });

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
        thumb !== storedThumb),
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
      pieces = piecesFromRanges(d.clip.removedRanges ?? [], d.durationMs);
      gameVolume = percent(d.clip.gameVolume);
      micVolume = percent(d.clip.micVolume);
      voiceVolume = percent(d.clip.voiceVolume ?? 1);
      masterVolume = percent(d.clip.masterVolume ?? 1);
      extraVolumes = Object.fromEntries(d.extraTracks.map((t) => [t.index, percent(d.clip.extraVolumes?.[t.index] ?? 1)]));
      blurVideo = d.clip.blurVideo ?? false;
      thumb = d.clip.thumbnailMs;
      position = trim.start;
      stripFrames = d.filmstrip.frames;
      data = d;
      stored = edit;
      storedThumb = thumb;
      void loadAudio(d);
    } catch (e) {
      loadError = errorMessage(e);
    }
  }

  /** The engine's key for each track, with the name the "couldn't load" toast uses. */
  function audioTracks(d: EditorData): { key: string; url: string; name: string }[] {
    return [
      { key: "game", url: d.gameAudioUrl, name: "game" },
      ...(d.micAudioUrl ? [{ key: "mic", url: d.micAudioUrl, name: "microphone" }] : []),
      ...(d.voiceAudioUrl ? [{ key: "voice", url: d.voiceAudioUrl, name: "voice chat" }] : []),
      ...d.extraTracks.map((t) => ({ key: `track-${t.index}`, url: t.audioUrl, name: `track ${t.index + 1}` })),
    ];
  }

  async function loadAudio(d: EditorData) {
    const tracks = audioTracks(d);
    let failed: string[];
    try {
      failed = await engine.load(tracks);
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
      cancelAnimationFrame(frame);
      stopProgress();
      stopFilmstrip();
      clearTimeout(settleTimer);
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
    scrubbing = true;
    wasPlaying = playing;
    if (playing) pause();
  }

  function onscrubend() {
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
    engine.play(ms, playRanges);
    playingRanges = playRanges;
    lastSegment = engine.segment();
    seekedPastCut = null;
    playing = true;
    if (lastSegment) {
      position = lastSegment.from;
      // Off by more than the drift the video catches up on by itself: move it.
      if (videoCorrection(video.currentTime * 1000, lastSegment.from, seekAverageMs, lastSegment.to)) {
        seekVideo(lastSegment.from);
      }
    }
    video.playbackRate = 1;
    video.play().catch(() => {});
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
    position = engine.positionMs() ?? position;
    engine.stop();
    playing = false;
    lastSegment = null;
    seekedPastCut = null;
    if (video) {
      video.pause();
      video.playbackRate = 1;
      // The video trails or leads the audio a little: show the playhead's own frame.
      if (!mediaError && Math.abs(video.currentTime * 1000 - position) > 20) seekVideo(position);
    }
  }

  /** Each frame while playing: the playhead follows the audio, and the video follows both. */
  function follow() {
    if (!video) return;
    if (silent) {
      followVideo();
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
    const next = splitAt(pieces, position, trim);
    if (next) pieces = next;
    else toast("Can't split here: too close to a split or the trim, or outside the trim", "error");
  }

  function onkeydown(e: KeyboardEvent) {
    const split = (e.key === "s" || e.key === "S") && !e.ctrlKey && !e.metaKey && !e.altKey;
    if ((e.key !== " " && !split) || e.defaultPrevented || uploadOpen) return;
    const target = e.target as HTMLElement | null;
    // Buttons and form fields use the space bar themselves.
    if (target?.closest("button, input, textarea, select, [contenteditable], [role='dialog']")) return;
    e.preventDefault();
    if (split) splitHere();
    else togglePlay();
  }

  async function saveEdit(): Promise<boolean> {
    if (!stored || !changed) return true;
    saving = true;
    const next = edit;
    const nextThumb = thumb === null ? null : clampToKept(thumb, kept);
    try {
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

  async function askDelete() {
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
      ondeleted();
    } catch (err) {
      toast(errorMessage(err), "error");
    } finally {
      deleting = false;
    }
  }

  async function onupload() {
    // The upload exports the saved edit, so save first: it uploads the cut the user sees.
    if (await saveEdit()) {
      pause();
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
      <button class="game" class:unknown={!gameName} onclick={setGame} title="Set game">
        {#if gameName}<span class="game-dot" style:--hue={hueOf(gameName)}></span>{/if}
        {gameName ?? "Unknown game"}
        <Icon name="chevronDown" size={13} />
      </button>
    </div>
    {#if changed}<span class="unsaved">Unsaved changes</span>{/if}
    <button
      class="btn btn-ghost delete"
      onclick={askDelete}
      disabled={uploading || deleting}
      title={uploading ? "Wait for the upload to finish" : "Delete this clip"}
    >
      <Icon name="trash" size={16} /> Delete
    </button>
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
    <div class="stage" bind:clientWidth={stageWidth} bind:clientHeight={stageHeight}>
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
      {#if mediaError}
        <div class="media-error">
          <Icon name="alert" size={28} />
          <p>Klipt can't play this clip here. You can still trim, export and upload it.</p>
        </div>
      {/if}
    </div>

    <div class="timeline">
      <div class="transport">
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
        <button
          class="btn btn-sm split-btn"
          onclick={splitHere}
          disabled={mediaError}
          title="Split at the playhead (S), then switch the parts you don't want off"
        >
          <Icon name="scissors" size={15} /> Split
        </button>
        <span class="clock"><strong>{formatClock(position)}</strong> / {formatClock(duration)}</span>
      </div>
      <TrimBar
        {duration}
        bind:start={trim.start}
        bind:end={trim.end}
        bind:thumb
        bind:pieces
        kept={keptLength(pieces, trim)}
        {position}
        strip={data ? { dir: data.filmstrip.dir, ready: stripFrames } : null}
        onseek={seek}
        {onhandlemove}
        {onscrubstart}
        {onscrubend}
      />
    </div>

    <div class="panel">
      <div class="mixer">
        <label class="blur-toggle" title="For clips where only the sound matters: uploads and exports blur the picture">
          <input type="checkbox" bind:checked={blurVideo} />
          <span class="switch" aria-hidden="true"></span>
          Blur video
        </label>
        <label class="volume master" title="The whole clip: every track together">
          <span class="vol-label"><Icon name="volume" size={17} /> Master</span>
          <input
            class="range"
            type="range"
            min="0"
            max="100"
            step="1"
            bind:value={masterVolume}
            style:--fill="{masterVolume}%"
            aria-label="Master volume"
          />
          <output>{masterVolume}%</output>
        </label>
        <label class="volume">
          <!-- An imported clip with more tracks: Klipt can't know which one is the game. -->
          <span class="vol-label"><Icon name="volume" size={17} /> {data.extraTracks.length > 0 ? "Track 1" : "Game"}</span>
          <input
            class="range"
            type="range"
            min="0"
            max="100"
            step="1"
            bind:value={gameVolume}
            style:--fill="{gameVolume}%"
            aria-label={data.extraTracks.length > 0 ? "Track 1 volume" : "Game volume"}
          />
          <output>{gameVolume}%</output>
        </label>
        {#if data.micAudioPath}
          <label class="volume">
            <span class="vol-label"><Icon name="mic" size={17} /> Mic</span>
            <input
              class="range"
              type="range"
              min="0"
              max="100"
              step="1"
              bind:value={micVolume}
              style:--fill="{micVolume}%"
              aria-label="Mic volume"
            />
            <output>{micVolume}%</output>
          </label>
        {/if}
        {#if data.voiceAudioPath}
          <label class="volume" title="Discord and other voice chat. Turn it down to share the clip without your friends' voices.">
            <span class="vol-label"><Icon name="users" size={17} /> Voice chat</span>
            <input
              class="range"
              type="range"
              min="0"
              max="100"
              step="1"
              bind:value={voiceVolume}
              style:--fill="{voiceVolume}%"
              aria-label="Voice chat volume"
            />
            <output>{voiceVolume}%</output>
          </label>
        {/if}
        {#each data.extraTracks as t (t.index)}
          <label class="volume">
            <span class="vol-label"><Icon name="volume" size={17} /> Track {t.index + 1}</span>
            <input
              class="range"
              type="range"
              min="0"
              max="100"
              step="1"
              bind:value={extraVolumes[t.index]}
              style:--fill="{extraVolumes[t.index]}%"
              aria-label="Track {t.index + 1} volume"
            />
            <output>{extraVolumes[t.index]}%</output>
          </label>
        {/each}
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
        <button class="btn" onclick={onsave} disabled={!changed || busy || !kept.length} title={kept.length ? undefined : nothingKept}>
          {#if saving}<span class="spinner"></span>{:else}<Icon name="check" size={16} />{/if}
          Save edit
        </button>
        <button class="btn" onclick={onexport} disabled={busy || !kept.length} title={kept.length ? undefined : nothingKept}>
          <Icon name="film" size={16} /> Export copy…
        </button>
        <button class="btn btn-primary" onclick={onupload} disabled={busy || !kept.length} title={kept.length ? undefined : nothingKept}>
          <Icon name="upload" size={16} /> Upload
        </button>
      </div>
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

  .delete {
    flex: none;
    margin-left: auto;
    color: var(--danger-text, var(--danger));
  }

  .unsaved + .delete {
    margin-left: 0;
  }

  .delete:hover:not(:disabled) {
    background: var(--danger-soft);
  }

  .unsaved {
    flex: none;
    margin-left: auto;
    padding: 0.2rem 0.65rem;
    border-radius: 999px;
    background: var(--warning-soft);
    color: var(--warning);
    font-size: 0.8rem;
    font-weight: 650;
    animation: rise 0.25s var(--ease-out);
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
    gap: 0.4rem;
    flex: none;
    width: 7.5rem;
  }

  .play {
    width: 3.5rem;
    height: 3.5rem;
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
    padding: 0.85rem 0.9rem 0.85rem 1.2rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
  }

  .mixer {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 2.25rem;
  }

  .blur-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.6rem;
    padding-right: 2.25rem;
    border-right: 1px solid var(--border);
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
  }

  .blur-toggle input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .switch {
    position: relative;
    flex: none;
    width: 2.1rem;
    height: 1.2rem;
    border-radius: 999px;
    background: var(--surface-3);
    transition: background 0.15s;
  }

  .switch::after {
    content: "";
    position: absolute;
    top: 0.15rem;
    left: 0.15rem;
    width: 0.9rem;
    height: 0.9rem;
    border-radius: 50%;
    background: var(--text-muted);
    transition:
      transform 0.15s var(--ease-out),
      background 0.15s;
  }

  .blur-toggle input:checked + .switch {
    background: var(--accent);
  }

  .blur-toggle input:checked + .switch::after {
    background: #fff;
    transform: translateX(0.9rem);
  }

  .blur-toggle input:focus-visible + .switch {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* Master sits apart from the per-track sliders. */
  .volume.master {
    padding-right: 2.25rem;
    border-right: 1px solid var(--border);
  }

  .volume {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    font-weight: 600;
  }

  .vol-label {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    white-space: nowrap;
    width: 4.6rem;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .volume .range {
    width: 10rem;
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
