// Plays every audio track through Web Audio on one clock, skipping removed parts exactly by
// scheduling each kept part as its own source. The editor follows `positionMs` with the video.

import { clipMsAt, schedule, segmentAt, type Segment, AUDIO_LEAD_MS } from "./playSchedule";
import type { TrimRange } from "./trim";
import { peaks, poolPeaks } from "./waveform";
import type { AudioRange } from "./api";
import { gainPoints } from "./audioParts";

/** The finest waveform kept per track; coarser ones are pooled from it (a multiple of 4096). */
export const FINE_PEAK_BUCKETS = 2 ** 18;

const LEAD_S = AUDIO_LEAD_MS / 1000;

export class AudioEngine {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  /** Per track: its samples, its fader for muted parts (`parts`) and its volume. */
  private tracks = new Map<string, { buffer: AudioBuffer; parts: GainNode; gain: GainNode }>();
  /** The muted and turned-down parts per track key (that track's own). */
  private partLists = new Map<string, AudioRange[]>();
  private peakCache = new Map<string, Float32Array>();
  /** Per track, the last part's peaks `peaksRange` made (one, so panning doesn't pile them up). */
  private windowCache = new Map<string, { id: string; peaks: Float32Array }>();
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
  async load(
    tracks: { key: string; url: string }[],
    onProgress?: (loaded: number, total: number | null) => void,
  ): Promise<string[]> {
    const ctx = this.context();
    // Bytes read and expected per track; the total is unknown until every size is.
    const loaded = tracks.map(() => 0);
    const sizes: (number | null)[] = tracks.map(() => null);
    const report = () => {
      const total = sizes.every((s) => s !== null) ? sizes.reduce<number>((a, s) => a + (s ?? 0), 0) : null;
      onProgress?.(loaded.reduce((a, b) => a + b, 0), total);
    };
    const results = await Promise.all(
      tracks.map(async ({ key, url }, i) => {
        try {
          const data = await readAll(await fetch(url), (bytes, size) => {
            loaded[i] = bytes;
            sizes[i] = size;
            report();
          });
          const buffer = await ctx.decodeAudioData(data);
          const gain = ctx.createGain();
          gain.connect(this.master!);
          const parts = ctx.createGain();
          parts.connect(gain);
          this.tracks.set(key, { buffer, parts, gain });
          for (const id of [...this.peakCache.keys()]) {
            if (id.startsWith(`${key}:`)) this.peakCache.delete(id);
          }
          this.windowCache.delete(key);
          return null;
        } catch {
          return key;
        }
      }),
    );
    return results.filter((k): k is string => k !== null);
  }

  /**
   * Waveform peaks of a loaded track, cached per key and bucket count; null if not loaded. The
   * samples are read once, into FINE_PEAK_BUCKETS; a count that divides it is pooled from that.
   */
  peaks(key: string, buckets: number): Float32Array | null {
    const track = this.tracks.get(key);
    if (!track) return null;
    const id = `${key}:${buckets}`;
    let cached = this.peakCache.get(id);
    if (!cached) {
      const channels = Array.from({ length: track.buffer.numberOfChannels }, (_, i) =>
        track.buffer.getChannelData(i),
      );
      const fineId = `${key}:${FINE_PEAK_BUCKETS}`;
      let fine = this.peakCache.get(fineId);
      if (!fine && FINE_PEAK_BUCKETS % buckets === 0) {
        fine = peaks(channels, FINE_PEAK_BUCKETS);
        this.peakCache.set(fineId, fine);
      }
      cached = (fine && poolPeaks(fine, buckets)) ?? peaks(channels, buckets);
      this.peakCache.set(id, cached);
    }
    return cached;
  }

  /**
   * Waveform peaks of part of a loaded track, the share `from`–`to` (0–1) of it, read straight
   * from the samples: detail for a zoomed-in timeline finer than FINE_PEAK_BUCKETS over the whole
   * track gives (long recordings). The last part per track is cached; null if not loaded.
   */
  peaksRange(key: string, from: number, to: number, buckets: number): Float32Array | null {
    const track = this.tracks.get(key);
    if (!track) return null;
    const id = `${from}:${to}:${buckets}`;
    const hit = this.windowCache.get(key);
    if (hit?.id === id) return hit.peaks;
    const channels = Array.from({ length: track.buffer.numberOfChannels }, (_, i) => track.buffer.getChannelData(i));
    const length = channels.reduce((max, c) => Math.max(max, c.length), 0);
    const start = Math.min(length, Math.max(0, Math.floor(from * length)));
    const end = Math.min(length, Math.max(start, Math.floor(to * length)));
    const result = peaks(
      channels.map((c) => c.subarray(start, end)),
      buckets,
    );
    this.windowCache.set(key, { id, peaks: result });
    return result;
  }

  setGain(key: string, gain: number) {
    const track = this.tracks.get(key);
    if (track) track.gain.gain.value = gain;
  }

  /**
   * The muted and turned-down parts of track `key` (its own only). While playing, the rest of the
   * playback follows them at once.
   */
  setParts(key: string, parts: AudioRange[]) {
    this.partLists.set(key, parts);
    const track = this.tracks.get(key);
    if (track) this.scheduleParts(track.parts, parts);
  }

  /**
   * Schedules `node`'s gain for `parts` over every part of the clip playing now, on the audio
   * clock: straight lines between the corners `gainPoints` gives, so it matches the export.
   */
  private scheduleParts(node: GainNode, parts: AudioRange[]) {
    const param = node.gain;
    param.cancelScheduledValues(0);
    if (!this.playing || parts.length === 0) {
      param.value = 1;
      return;
    }
    for (const seg of this.segments) {
      const at = (ms: number) => this.startedAt + seg.at + (ms - seg.from) / 1000;
      const points = gainPoints(parts, seg.from, seg.to);
      param.setValueAtTime(points[0].gain, at(points[0].ms));
      for (const point of points.slice(1)) param.linearRampToValueAtTime(point.gain, at(point.ms));
    }
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
      for (const { buffer, parts } of this.tracks.values()) {
        const source = ctx.createBufferSource();
        source.buffer = buffer;
        source.connect(parts);
        source.start(this.startedAt + seg.at, seg.from / 1000, (seg.to - seg.from) / 1000);
        this.sources.push(source);
      }
    }
    this.playing = true;
    for (const [key, track] of this.tracks) this.scheduleParts(track.parts, this.partLists.get(key) ?? []);
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
    this.partLists.clear();
    this.peakCache.clear();
    this.windowCache.clear();
  }
}

/** A response's body, reporting the bytes read so far and its size (null when not given). */
async function readAll(response: Response, progress: (bytes: number, size: number | null) => void): Promise<ArrayBuffer> {
  const size = Number(response.headers.get("content-length")) || null;
  const reader = response.body?.getReader();
  if (!reader) {
    const data = await response.arrayBuffer();
    progress(data.byteLength, data.byteLength);
    return data;
  }
  const parts: Uint8Array[] = [];
  let bytes = 0;
  progress(0, size);
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    parts.push(value);
    bytes += value.byteLength;
    progress(bytes, size === null ? null : Math.max(size, bytes));
  }
  const data = new Uint8Array(bytes);
  let at = 0;
  for (const part of parts) {
    data.set(part, at);
    at += part.byteLength;
  }
  return data.buffer;
}
