<script lang="ts">
  import { clockLabel, nextSpeed, savePlayerPrefs } from "../lib/player";
  import Icon from "./Icon.svelte";

  // The clip player's bar over the bottom of the video: seek, play, volume, time, speed and
  // fullscreen. It reads and drives `video` itself; the player decides when it shows.
  let {
    video,
    fullscreen,
    visible,
    ontogglefullscreen,
    onfocuschange,
  }: {
    video: HTMLVideoElement;
    fullscreen: boolean;
    visible: boolean;
    ontogglefullscreen: () => void;
    /** True while a control has keyboard focus or the seek bar is dragged: keep the bar shown. */
    onfocuschange: (focused: boolean) => void;
  } = $props();

  let paused = $state(true);
  let current = $state(0);
  let duration = $state(0);
  let buffered = $state(0);
  let volume = $state(1);
  let muted = $state(false);
  let rate = $state(1);
  let dragging = $state(false);

  $effect(() => {
    const v = video;
    const sync = () => {
      paused = v.paused;
      if (!dragging) current = v.currentTime;
      duration = Number.isFinite(v.duration) ? v.duration : 0;
      buffered = v.buffered.length ? v.buffered.end(v.buffered.length - 1) : 0;
      volume = v.volume;
      muted = v.muted;
      rate = v.playbackRate;
    };
    const events = ["play", "pause", "timeupdate", "durationchange", "loadedmetadata", "progress", "volumechange", "ratechange", "emptied", "seeked"];
    events.forEach((e) => v.addEventListener(e, sync));
    sync();
    return () => events.forEach((e) => v.removeEventListener(e, sync));
  });

  const played = $derived(duration > 0 ? Math.min(1, current / duration) : 0);
  const loaded = $derived(duration > 0 ? Math.min(1, buffered / duration) : 0);

  function toggle() {
    if (video.paused) video.play().catch(() => {});
    else video.pause();
  }

  let track = $state<HTMLDivElement>();
  function seekTo(e: PointerEvent) {
    if (!track || duration <= 0) return;
    const box = track.getBoundingClientRect();
    const at = Math.min(1, Math.max(0, (e.clientX - box.left) / box.width)) * duration;
    current = at;
    video.currentTime = at;
  }
  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    track?.setPointerCapture(e.pointerId);
    dragging = true;
    onfocuschange(true);
    seekTo(e);
  }
  function onpointermove(e: PointerEvent) {
    if (dragging) seekTo(e);
  }
  function onpointerup(e: PointerEvent) {
    if (!dragging) return;
    track?.releasePointerCapture(e.pointerId);
    dragging = false;
    onfocuschange(false);
  }
  function onseekkey(e: KeyboardEvent) {
    // The seek bar's own arrows would fight the player's (previous / next clip): Home / End only.
    if (e.key === "Home") video.currentTime = 0;
    else if (e.key === "End" && duration > 0) video.currentTime = duration;
  }

  function setVolume(value: number) {
    video.volume = value;
    video.muted = value === 0;
    savePlayerPrefs({ volume: value, muted: video.muted });
  }
  function toggleMute() {
    video.muted = !video.muted;
    if (!video.muted && video.volume === 0) video.volume = 1;
    savePlayerPrefs({ volume: video.volume, muted: video.muted });
  }
</script>

<div
  class="bar"
  class:hidden={!visible}
  onfocusin={() => onfocuschange(true)}
  onfocusout={() => onfocuschange(false)}
  role="group"
  aria-label="Player controls"
>
  <div
    class="seek"
    class:dragging
    bind:this={track}
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
    onkeydown={onseekkey}
    role="slider"
    tabindex="0"
    aria-label="Seek"
    aria-valuemin={0}
    aria-valuemax={Math.round(duration)}
    aria-valuenow={Math.round(current)}
    aria-valuetext={`${clockLabel(current)} of ${clockLabel(duration)}`}
  >
    <div class="rail">
      <div class="loaded" style:width="{loaded * 100}%"></div>
      <div class="played" style:width="{played * 100}%"></div>
    </div>
    <div class="knob" style:left="{played * 100}%"></div>
  </div>
  <div class="row">
    <button class="ctl" onclick={toggle} aria-label={paused ? "Play" : "Pause"} title={paused ? "Play (Space)" : "Pause (Space)"}>
      <Icon name={paused ? "play" : "pause"} size={20} />
    </button>
    <button class="ctl" onclick={toggleMute} aria-label={muted ? "Unmute" : "Mute"} title={muted ? "Unmute (M)" : "Mute (M)"}>
      <Icon name={muted || volume === 0 ? "volumeOff" : "volume"} size={20} />
    </button>
    <input
      class="volume"
      type="range"
      min="0"
      max="1"
      step="0.01"
      value={muted ? 0 : volume}
      style:--level="{(muted ? 0 : volume) * 100}%"
      oninput={(e) => setVolume(Number(e.currentTarget.value))}
      aria-label="Volume"
    />
    <span class="time">{clockLabel(current)} / {clockLabel(duration)}</span>
    <span class="spacer"></span>
    <button class="ctl speed" onclick={() => (video.playbackRate = nextSpeed(rate))} aria-label="Speed {rate}×" title="Playback speed">
      {rate}×
    </button>
    <button
      class="ctl"
      onclick={ontogglefullscreen}
      aria-label={fullscreen ? "Leave fullscreen" : "Fullscreen"}
      title={fullscreen ? "Leave fullscreen (F)" : "Fullscreen (F)"}
    >
      <Icon name={fullscreen ? "fullscreenExit" : "fullscreen"} size={20} />
    </button>
  </div>
</div>

<style>
  .bar {
    position: absolute;
    inset: auto 0 0 0;
    padding: 40px 14px 10px;
    background: linear-gradient(transparent, rgba(0, 0, 0, 0.75));
    color: #fff;
    transition: opacity 200ms ease;
  }
  .bar.hidden {
    opacity: 0;
    pointer-events: none;
  }
  .seek {
    position: relative;
    height: 16px;
    display: flex;
    align-items: center;
    cursor: pointer;
    touch-action: none;
    outline: none;
  }
  .rail {
    position: relative;
    width: 100%;
    height: 4px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.18);
    overflow: hidden;
    transition: height 120ms ease;
  }
  .seek:hover .rail,
  .seek.dragging .rail,
  .seek:focus-visible .rail {
    height: 6px;
  }
  .loaded,
  .played {
    position: absolute;
    inset: 0 auto 0 0;
  }
  .loaded {
    background: rgba(255, 255, 255, 0.3);
  }
  .played {
    background: var(--accent);
  }
  .knob {
    position: absolute;
    top: 50%;
    width: 12px;
    height: 12px;
    margin: -6px 0 0 -6px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 0 0 3px var(--accent-soft);
    opacity: 0;
    transition: opacity 120ms ease;
    pointer-events: none;
  }
  .seek:hover .knob,
  .seek.dragging .knob,
  .seek:focus-visible .knob {
    opacity: 1;
  }
  .seek:focus-visible {
    box-shadow: 0 0 0 2px var(--accent-soft);
    border-radius: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
  }
  .ctl {
    display: grid;
    place-items: center;
    min-width: 34px;
    height: 34px;
    padding: 0 6px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .ctl:hover {
    background: rgba(255, 255, 255, 0.12);
  }
  .ctl:focus-visible {
    outline: 2px solid var(--accent);
  }
  .speed {
    font-size: 13px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .volume {
    width: 72px;
    height: 4px;
    margin: 0 6px 0 0;
    appearance: none;
    border-radius: 4px;
    background: linear-gradient(90deg, #fff var(--level), rgba(255, 255, 255, 0.25) var(--level));
    cursor: pointer;
  }
  .volume::-webkit-slider-thumb {
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
  }
  .volume:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 4px;
  }
  .time {
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
    color: rgba(255, 255, 255, 0.9);
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
</style>
