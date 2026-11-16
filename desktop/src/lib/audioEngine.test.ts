import { afterEach, describe, expect, it, vi } from "vitest";
import { AudioEngine } from "./audioEngine";

const skip = [
  { start: 0, end: 2000 },
  { start: 3000, end: 5000 },
];

function fakeContext() {
  const starts: { url: string; when: number; offset: number; duration: number }[] = [];
  const stops: string[] = [];
  const gains: { gain: { value: number }; connected: unknown }[] = [];
  const ctx = {
    currentTime: 10,
    destination: { dest: true },
    resume: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    decodeAudioData: async (data: ArrayBuffer) => {
      const url = new TextDecoder().decode(data);
      if (url === "bad") throw new Error("decode");
      return {
        url,
        numberOfChannels: 1,
        getChannelData: () =>
          new Float32Array(url === "loud" ? [0.8, 0, 0, 0] : [0, 0.5, -1, 0.25]),
      };
    },
    createGain() {
      const g = { gain: { value: 1 }, connected: null as unknown, connect(n: unknown) { g.connected = n; } };
      gains.push(g);
      return g;
    },
    createBufferSource() {
      const s = {
        buffer: null as { url: string } | null,
        connect() {},
        start(when: number, offset: number, duration: number) {
          starts.push({ url: s.buffer!.url, when, offset, duration });
        },
        stop() {
          stops.push(s.buffer!.url);
        },
      };
      return s;
    },
  };
  return { ctx, starts, stops, gains };
}

function stubFetch() {
  vi.stubGlobal("fetch", async (url: string) => {
    if (url === "bad") return { arrayBuffer: async () => new TextEncoder().encode("bad").buffer };
    return { arrayBuffer: async () => new TextEncoder().encode(url).buffer };
  });
}

async function loaded(keys = ["game", "mic"], urls: Record<string, string> = {}) {
  stubFetch();
  const f = fakeContext();
  const engine = new AudioEngine(() => f.ctx as unknown as AudioContext);
  const failed = await engine.load(keys.map((key) => ({ key, url: urls[key] ?? key })));
  return { ...f, engine, failed };
}

afterEach(() => vi.unstubAllGlobals());

describe("AudioEngine", () => {
  it("skips a track that fails to load", async () => {
    const { engine, failed, starts } = await loaded(["game", "mic"], { mic: "bad" });
    expect(failed).toEqual(["mic"]);
    engine.setGain("mic", 0.3);
    engine.play(1500, skip);
    expect(starts.length).toBe(2);
    expect(starts.every((s) => s.url === "game")).toBe(true);
  });

  it("schedules one source per track per segment", async () => {
    const { engine, starts } = await loaded();
    engine.play(1500, skip);
    expect(starts.length).toBe(4);
    expect(engine.playing).toBe(true);
    const game = starts.filter((s) => s.url === "game");
    expect(game[0]).toMatchObject({ offset: 1.5, duration: 0.5 });
    expect(game[0].when).toBeCloseTo(10.05);
    expect(game[1]).toMatchObject({ offset: 3, duration: 2 });
    expect(game[1].when).toBeCloseTo(10.55);
  });

  it("sets gain values", async () => {
    const { engine, gains } = await loaded();
    engine.setGain("game", 0.5);
    engine.setMaster(0.8);
    const values = gains.map((g) => g.gain.value);
    expect(values).toContain(0.5);
    expect(values).toContain(0.8);
  });

  it("stops every source", async () => {
    const { engine, stops } = await loaded();
    engine.play(0, skip);
    engine.stop();
    expect(stops.length).toBe(4);
    expect(engine.playing).toBe(false);
    expect(engine.positionMs()).toBeNull();
    expect(engine.segment()).toBeNull();
  });

  it("reports the position and segment on the audio clock", async () => {
    const { engine, ctx } = await loaded();
    engine.play(1500, skip);
    ctx.currentTime = 10.05 + 0.25;
    expect(engine.positionMs()).toBe(1750);
    expect(engine.segment()).toEqual({ at: 0, from: 1500, to: 2000 });
    ctx.currentTime = 10.05 + 0.75;
    expect(engine.positionMs()).toBe(3250);
    expect(engine.segment()).toEqual({ at: 0.5, from: 3000, to: 5000 });
    ctx.currentTime = 10.05 + 3;
    expect(engine.positionMs()).toBeNull();
    expect(engine.segment()).toBeNull();
  });

  it("tells the part that plays after this one", async () => {
    const { engine, ctx } = await loaded();
    expect(engine.nextSegment()).toBeNull();
    engine.play(1500, skip);
    ctx.currentTime = 10.05 + 0.25;
    expect(engine.nextSegment()).toEqual({ at: 0.5, from: 3000, to: 5000 });
    ctx.currentTime = 10.05 + 0.75;
    expect(engine.nextSegment()).toBeNull();
  });
});

describe("peaks", () => {
  it("computes peaks from the decoded track", async () => {
    const { engine } = await loaded();
    expect(Array.from(engine.peaks("game", 2)!)).toEqual([0.5, 1]);
  });

  it("caches per key and bucket count", async () => {
    const { engine } = await loaded();
    expect(engine.peaks("game", 2)).toBe(engine.peaks("game", 2));
    expect(engine.peaks("game", 4)).not.toBe(engine.peaks("game", 2));
  });

  it("recomputes when the key is reloaded with a different buffer", async () => {
    const { engine } = await loaded();
    const old = engine.peaks("game", 2)!;
    await engine.load([{ key: "game", url: "loud" }]);
    const fresh = engine.peaks("game", 2)!;
    expect(fresh).not.toBe(old);
    expect(Array.from(fresh)).toEqual([Math.fround(0.8), 0]);
  });

  it("forgets tracks and cached peaks on close", async () => {
    const { engine } = await loaded();
    const old = engine.peaks("game", 2)!;
    engine.close();
    // Not observable through the public API: checks the cached arrays' memory is released.
    expect((engine as unknown as { peakCache: Map<string, unknown> }).peakCache.size).toBe(0);
    expect(engine.peaks("game", 2)).toBeNull();
    await engine.load([{ key: "game", url: "game" }]);
    const fresh = engine.peaks("game", 2)!;
    expect(fresh).not.toBe(old);
    expect(Array.from(fresh)).toEqual([0.5, 1]);
  });

  it("is null for a key that is not loaded", async () => {
    const { engine } = await loaded();
    expect(engine.peaks("nope", 2)).toBeNull();
  });
});
