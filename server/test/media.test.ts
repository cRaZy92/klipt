import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  MediaError,
  assertPlayable,
  makeThumbnail,
  probeVideo,
  remuxFaststart,
} from '../src/clips/media.js';
import { makeFixtureVideo } from './helpers.js';

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
});
