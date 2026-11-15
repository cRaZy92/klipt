import { execFile } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { EmbedBackfill, embedPlan } from '../src/clips/embed.js';
import { probeVideo } from '../src/clips/media.js';
import { createUser, makeTestApp, sessionFor, tusUpload, type TestApp } from './helpers.js';

/** A 2 s H.264 + AAC clip at the given size and frame rate. */
async function makeVideo(dir: string, size: string, rate: number): Promise<string> {
  const out = join(dir, `src-${size}-${rate}.mp4`);
  const args = ['-v', 'error', '-y', '-f', 'lavfi', '-i', `testsrc2=size=${size}:rate=${rate}`, '-f', 'lavfi', '-i', 'sine=frequency=440'];
  args.push('-t', '2', '-c:v', 'libx264', '-preset', 'ultrafast', '-crf', '10', '-pix_fmt', 'yuv420p', '-c:a', 'aac', out);
  await new Promise<void>((resolve, reject) =>
    execFile(process.env.FFMPEG_PATH ?? 'ffmpeg', args, (err, _o, stderr) => (err ? reject(new Error(stderr || err.message)) : resolve())),
  );
  return out;
}

describe('embedPlan', () => {
  it('a short 1080p clip gets a 720p copy at the full bitrate', () => {
    expect(embedPlan({ width: 1920, height: 1080, durationMs: 12_000 })).toEqual({ width: 1280, height: 720, videoKbps: 2500 });
  });

  it('a longer clip gets a lower bitrate and a smaller picture, so the copy stays about 8 MB', () => {
    const plan = embedPlan({ width: 1920, height: 1080, durationMs: 60_000 })!;
    expect(plan).toMatchObject({ width: 960, height: 540 });
    expect(plan.videoKbps).toBeGreaterThan(900);
    expect(((plan.videoKbps + 96) * 60) / 8 / 1024).toBeLessThan(8 * 1024 * 0.96);
  });

  it('keeps a small picture as it is, and portrait clips upright', () => {
    expect(embedPlan({ width: 640, height: 360, durationMs: 5000 })).toMatchObject({ width: 640, height: 360 });
    expect(embedPlan({ width: 1080, height: 1920, durationMs: 5000 })).toMatchObject({ width: 720, height: 1280 });
  });

  it('a clip too long for a decent copy has none', () => {
    expect(embedPlan({ width: 1920, height: 1080, durationMs: 120_000 })).toBeNull();
  });
});

describe('embed copies', () => {
  let t: TestApp;
  let dir: string;
  let token: string;
  let aliceWeb: Record<string, string>;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'klipt-embed-'));
    t = await makeTestApp();
    const alice = await createUser(t.db, { username: 'alice' });
    token = (await sessionFor(t.db, alice.id, 'desktop')).token;
    aliceWeb = (await sessionFor(t.db, alice.id)).headers;
  });
  afterEach(async () => {
    await t.close();
    await rm(dir, { recursive: true, force: true });
  });

  const upload = async (file: string) => {
    const { uploadId } = await tusUpload(t.app, token, await readFile(file));
    const res = await t.app.inject({
      method: 'POST',
      url: '/api/clips',
      headers: { authorization: `Bearer ${token}` },
      payload: { uploadId, title: 'Ace' },
    });
    expect(res.statusCode).toBe(201);
    return res.json() as { id: string };
  };
  const clipsDir = () => join(t.dataDir, 'clips');

  it('an upload gets its embed copy right away, and the share page points Discord at it', async () => {
    const { id } = await upload(await makeVideo(dir, '1280x720', 60));
    const file = join(clipsDir(), `${id}.embed.mp4`);
    expect(existsSync(file)).toBe(true);
    const info = await probeVideo(file);
    expect(info).toMatchObject({ videoCodec: 'h264', audioCodec: 'aac', width: 1280, height: 720 });
    expect(info.fps).toBe(30);
    expect(Math.abs(info.durationMs - 2000)).toBeLessThan(150);
    const bytes = await readFile(file);
    expect(bytes.indexOf('moov')).toBeLessThan(bytes.indexOf('mdat'));

    const page = await t.app.inject({ url: `/c/${id}` });
    expect(page.body).toContain(`<meta property="og:video" content="http://localhost/media/${id}.embed.mp4"`);
    expect(page.body).toContain('<meta property="og:video:width" content="1280"');
    // The page's own player keeps the full video.
    expect(page.body).toContain(`src="http://localhost/media/${id}.mp4"`);

    const media = await t.app.inject({ url: `/media/${id}.embed.mp4` });
    expect(media.statusCode).toBe(200);
    expect(media.headers['content-type']).toBe('video/mp4');
  });

  it('without a copy the share page embeds the full video', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    await rm(join(clipsDir(), `${id}.embed.mp4`));
    const page = await t.app.inject({ url: `/c/${id}` });
    expect(page.body).toContain(`<meta property="og:video" content="http://localhost/media/${id}.mp4"`);
    // A missing copy is a 404, never the full video under its name.
    expect((await t.app.inject({ url: `/media/${id}.embed.mp4` })).statusCode).toBe(404);
  });

  it('deleting a clip removes its embed copy', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    const del = await t.app.inject({ method: 'DELETE', url: `/api/clips/${id}`, headers: aliceWeb });
    expect(del.statusCode).toBe(204);
    expect(existsSync(join(clipsDir(), `${id}.embed.mp4`))).toBe(false);
  });

  it('the backfill makes copies for clips that have none', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    await rm(join(clipsDir(), `${id}.embed.mp4`));
    const backfill = new EmbedBackfill({ db: t.db, clipsDir: clipsDir(), threads: 2, log: t.app.log });
    backfill.start();
    await backfill.idle();
    expect(existsSync(join(clipsDir(), `${id}.embed.mp4`))).toBe(true);
  });
});

describe('embed copies with compression on', () => {
  let t: TestApp;
  let dir: string;
  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'klipt-embed-'));
    t = await makeTestApp({ transcode: true });
    await t.app.ready();
  });
  afterEach(async () => {
    await t.close();
    await rm(dir, { recursive: true, force: true });
  });

  it('survive compression and the leftover-file sweep', async () => {
    const alice = await createUser(t.db, { username: 'alice' });
    const token = (await sessionFor(t.db, alice.id, 'desktop')).token;
    const { uploadId } = await tusUpload(t.app, token, await readFile(await makeVideo(dir, '640x360', 30)));
    const res = await t.app.inject({
      method: 'POST',
      url: '/api/clips',
      headers: { authorization: `Bearer ${token}` },
      payload: { uploadId, title: 'Ace' },
    });
    const { id } = res.json() as { id: string };
    await t.app.transcoder.idle();
    await t.app.embedBackfill!.idle();
    await t.app.transcoder.start(); // what a restart does: sweep leftover files
    await t.app.transcoder.idle();
    expect(existsSync(join(t.dataDir, 'clips', `${id}.embed.mp4`))).toBe(true);
  });
});
