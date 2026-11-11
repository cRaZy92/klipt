import { chmod, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  MediaError,
  assertPlayable,
  makeThumbnail,
  thumbnailAtMs,
  probeVideo,
  remuxFaststart,
} from '../src/clips/media.js';
import { makeCorruptVideo, makeFixtureVideo } from './helpers.js';

let dir: string;
beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), 'klipt-media-'));
});
afterEach(async () => {
  await rm(dir, { recursive: true, force: true });
});

describe('media', () => {
  it('probes a fixture video', async () => {
    const info = await probeVideo(await makeFixtureVideo(dir));
    expect(Math.abs(info.durationMs - 2000)).toBeLessThanOrEqual(100);
    expect(info).toMatchObject({ width: 320, height: 240, videoCodec: 'h264', audioCodec: 'aac' });
    expect(() => assertPlayable(info)).not.toThrow();
  });

  it('rejects a text file as not_a_video', async () => {
    const p = join(dir, 'x.mp4');
    await writeFile(p, 'hello world');
    await expect(probeVideo(p)).rejects.toMatchObject({ code: 'not_a_video' });
    await expect(probeVideo(p)).rejects.toBeInstanceOf(MediaError);
  });

  it('rejects non-h264 as unsupported_codec', async () => {
    const info = await probeVideo(await makeFixtureVideo(dir, { codec: 'mpeg4' }));
    expect(() => assertPlayable(info)).toThrowError(
      expect.objectContaining({ code: 'unsupported_codec' }),
    );
  });

  it('accepts video without audio', async () => {
    const info = await probeVideo(await makeFixtureVideo(dir, { audio: false }));
    expect(info.audioCodec).toBeNull();
    expect(() => assertPlayable(info)).not.toThrow();
  });

  it('remux puts moov before mdat', async () => {
    const src = await makeFixtureVideo(dir);
    const before = await readFile(src);
    expect(before.indexOf('moov')).toBeGreaterThan(before.indexOf('mdat'));
    const out = join(dir, 'out.mp4');
    await remuxFaststart(src, out);
    const after = await readFile(out);
    expect(after.indexOf('moov')).toBeLessThan(after.indexOf('mdat'));
  });

  it('makes a JPEG thumbnail', async () => {
    const src = await makeFixtureVideo(dir);
    const out = join(dir, 'thumb.jpg');
    await makeThumbnail(src, out, 2000);
    const buf = await readFile(out);
    expect([buf[0], buf[1]]).toEqual([0xff, 0xd8]);
  });

  it('maps a non-zero ffmpeg exit on a damaged file to corrupt_video', async () => {
    const src = await makeCorruptVideo(dir);
    const info = await probeVideo(src);
    expect(() => assertPlayable(info)).not.toThrow();
    const remuxed = join(dir, 'remuxed.mp4');
    await remuxFaststart(src, remuxed);
    const err = await makeThumbnail(remuxed, join(dir, 't.jpg'), info.durationMs).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(MediaError);
    expect(err).toMatchObject({ code: 'corrupt_video' });
  });

  it('treats a full disk as a server problem, not a corrupt video', async () => {
    const src = await makeFixtureVideo(dir);
    const fake = join(dir, 'ffmpeg');
    await writeFile(fake, '#!/bin/sh\necho "av_interleaved_write_frame(): No space left on device" >&2\nexit 1\n');
    await chmod(fake, 0o755);
    const saved = process.env.FFMPEG_PATH;
    try {
      process.env.FFMPEG_PATH = fake;
      const err = await remuxFaststart(src, join(dir, 'o.mp4')).catch((e: unknown) => e);
      expect(err).toBeInstanceOf(Error);
      expect(err).not.toBeInstanceOf(MediaError);
      expect(String((err as Error).message)).toContain('No space left on device');
    } finally {
      if (saved === undefined) delete process.env.FFMPEG_PATH;
      else process.env.FFMPEG_PATH = saved;
    }
  });

  it('reports a missing binary as a non-media error', async () => {
    const src = await makeFixtureVideo(dir);
    const saved = { probe: process.env.FFPROBE_PATH, mpeg: process.env.FFMPEG_PATH };
    try {
      process.env.FFPROBE_PATH = '/nonexistent/ffprobe';
      process.env.FFMPEG_PATH = '/nonexistent/ffmpeg';
      const probeErr = await probeVideo(src).catch((e: unknown) => e);
      expect(probeErr).toBeInstanceOf(Error);
      expect(probeErr).not.toBeInstanceOf(MediaError);
      const remuxErr = await remuxFaststart(src, join(dir, 'o.mp4')).catch((e: unknown) => e);
      expect(remuxErr).toBeInstanceOf(Error);
      expect(remuxErr).not.toBeInstanceOf(MediaError);
    } finally {
      for (const [k, v] of [['FFPROBE_PATH', saved.probe], ['FFMPEG_PATH', saved.mpeg]] as const) {
        if (v === undefined) delete process.env[k];
        else process.env[k] = v;
      }
    }
  });
});

describe('thumbnailAtMs', () => {
  it('uses the chosen frame inside the video, else 1 s in', () => {
    expect(thumbnailAtMs(30_000)).toBe(1000);
    expect(thumbnailAtMs(1_000)).toBe(500);
    expect(thumbnailAtMs(30_000, 12_345)).toBe(12_345);
    expect(thumbnailAtMs(30_000, null)).toBe(1000);
    expect(thumbnailAtMs(30_000, 99_000)).toBe(29_900);
    expect(thumbnailAtMs(30_000, -5)).toBe(0);
  });
});
