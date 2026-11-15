// Plays every audio track through Web Audio on one clock, skipping removed parts exactly by
// scheduling each kept part as its own source. The editor follows `positionMs` with the video.

import { clipMsAt, schedule, segmentAt, type Segment } from "./playSchedule";
import type { TrimRange } from "./trim";

const LEAD_S = 0.05;

export class AudioEngine {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private tracks = new Map<string, { buffer: AudioBuffer; gain: GainNode }>();
  private sources: AudioBufferSourceNode[] = [];
  private segments: Segment[] = [];
  private startedAt = 0;
  playing = false;

  constructor(private makeContext: () => AudioContext = () => new AudioContext()) {}

  private context(): AudioContext {
    if (!this.ctx) {
      this.ctx = this.makeContext();
      this.master = this.ctx.createGain();
      this.master.connect(this.ctx.destination);
    }
    return this.ctx;
  }

  /** Loads the tracks; returns the keys that failed (they are skipped when playing). */
  async load(tracks: { key: string; url: string }[]): Promise<string[]> {
    const ctx = this.context();
    const results = await Promise.all(
      tracks.map(async ({ key, url }) => {
        try {
          const data = await (await fetch(url)).arrayBuffer();
          const buffer = await ctx.decodeAudioData(data);
          const gain = ctx.createGain();
          gain.connect(this.master!);
          this.tracks.set(key, { buffer, gain });
          return null;
        } catch {
          return key;
        }
      }),
    );
    return results.filter((k): k is string => k !== null);
  }

  setGain(key: string, gain: number) {
    const track = this.tracks.get(key);
    if (track) track.gain.gain.value = gain;
  }

  setMaster(gain: number) {
    this.context();
    this.master!.gain.value = gain;
  }

  play(fromMs: number, kept: TrimRange[]) {
    this.stop();
    const ctx = this.context();
    void ctx.resume();
    this.segments = schedule(fromMs, kept);
    this.startedAt = ctx.currentTime + LEAD_S;
    for (const seg of this.segments) {
      for (const { buffer, gain } of this.tracks.values()) {
        const source = ctx.createBufferSource();
        source.buffer = buffer;
        source.connect(gain);
        source.start(this.startedAt + seg.at, seg.from / 1000, (seg.to - seg.from) / 1000);
        this.sources.push(source);
      }
    }
    this.playing = true;
  }

  stop() {
    for (const source of this.sources) {
      try {
        source.stop();
      } catch {
        // already ended
      }
    }
    this.sources = [];
    this.segments = [];
    this.playing = false;
  }

  /** The clip position on the audio clock; null when stopped or past the end. */
  positionMs(): number | null {
    if (!this.playing || !this.ctx) return null;
    return clipMsAt(this.ctx.currentTime - this.startedAt, this.segments);
  }

  /** The segment playing now; null when stopped or past the end. */
  segment(): Segment | null {
    if (!this.playing || !this.ctx) return null;
    return segmentAt(this.ctx.currentTime - this.startedAt, this.segments);
  }

  /** The part that plays after the one playing now; null when stopped or on the last part. */
  nextSegment(): Segment | null {
    const seg = this.segment();
    return seg ? (this.segments[this.segments.indexOf(seg) + 1] ?? null) : null;
  }

  close() {
    this.stop();
    void this.ctx?.close();
    this.ctx = null;
    this.master = null;
    this.tracks.clear();
  }
}
