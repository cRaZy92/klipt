import { describe, expect, it } from "vitest";
import { AudioSync, type Media } from "./audioSync";

/** The muted video: its time runs while it plays. */
class FakeVideo implements Media {
  currentTime = 100;
  paused = false;
  seeking = false;
  run(dt: number) {
    if (!this.paused) this.currentTime += dt;
  }
}

/**
 * An <audio> in WebKitGTK: moved while playing, it still says it's playing but its time stays put
 * for `restart` seconds while it starts again (what the user's log shows).
 */
class FakeTrack implements Media {
  paused = false;
  seeking = false;
  moves = 0;
  /** Seconds of sound actually played (jumps don't count). */
  played = 0;
  private time = 0;
  private frozen = 0;
  constructor(private restart: number) {}
  get currentTime() {
    return this.time;
  }
  set currentTime(t: number) {
    this.moves++;
    this.time = t;
    if (!this.paused) this.frozen = this.restart;
  }
  run(dt: number) {
    if (this.paused) return;
    const ran = Math.max(0, dt - this.frozen);
    this.frozen = Math.max(0, this.frozen - dt);
    this.time += ran;
    this.played += ran;
  }
}

/** Plays for `seconds`, checking every 250 ms like the editor, and reports how the track fared. */
function play(restart: number, seconds = 10) {
  let now = 0;
  const sync = new AudioSync(() => now);
  const video = new FakeVideo();
  const track = new FakeTrack(restart);
  sync.align(video, [track]); // playback starts (or a seek lands)
  const step = 0.01;
  let stalledWindows = 0;
  let windows = 0;
  let windowStart = track.played;
  for (let i = 1; i <= seconds / step; i++) {
    now += step * 1000;
    video.run(step);
    track.run(step);
    if (i % 25 === 0) sync.check(video, [track]);
    if (i % 50 === 0 && now > 2000) {
      // Every 0.5 s after the first 2 s: did the track move forward?
      windows++;
      if (track.played <= windowStart) stalledWindows++;
    }
    if (i % 50 === 0) windowStart = track.played;
  }
  return { stalledWindows, windows, moves: track.moves, offset: track.currentTime - video.currentTime };
}

describe("AudioSync", () => {
  it("leaves a track that keeps up alone", () => {
    const r = play(0);
    expect(r.moves).toBe(1); // just the first alignment
    expect(Math.abs(r.offset)).toBeLessThan(0.01);
  });

  it("doesn't keep restarting a track that is slow to start again (the silent editor)", () => {
    const r = play(0.3);
    expect(r.stalledWindows).toBe(0);
    expect(Math.abs(r.offset)).toBeLessThan(0.05);
  });

  it("gets a slow track in step and then stops moving it", () => {
    const r = play(0.3, 20);
    expect(r.moves).toBeLessThanOrEqual(4);
    expect(Math.abs(r.offset)).toBeLessThan(0.05);
  });
});
