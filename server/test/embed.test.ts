import { execFile } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { eq } from 'drizzle-orm';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { EMBED_VERSION, EmbedBackfill, embedPlan } from '../src/clips/embed.js';
import { shareReadyOf } from '../src/clips/routes.js';
import { embedFileName } from '../src/clips/files.js';
import { probeVideo } from '../src/clips/media.js';
import { clips } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, tusUpload, type TestApp } from './helpers.js';

/** A H.264 + AAC clip at the given size and frame rate, 2 s long unless said otherwise. */
async function makeVideo(dir: string, size: string, rate: number, seconds = 2): Promise<string> {
  const out = join(dir, `src-${size}-${rate}-${seconds}.mp4`);
  const args = ['-v', 'error', '-y', '-f', 'lavfi', '-i', `testsrc2=size=${size}:rate=${rate}`, '-f', 'lavfi', '-i', 'sine=frequency=440'];
  args.push('-t', String(seconds), '-c:v', 'libx264', '-preset', 'ultrafast', '-crf', '10', '-pix_fmt', 'yuv420p', '-c:a', 'aac', out);
  await new Promise<void>((resolve, reject) =>
    execFile(process.env.FFMPEG_PATH ?? 'ffmpeg', args, (err, _o, stderr) => (err ? reject(new Error(stderr || err.message)) : resolve())),
  );
  return out;
}

describe('embedPlan', () => {
  it('a short 1080p clip gets a 720p copy that spends its whole 8 MB on picture', () => {
    const plan = embedPlan({ width: 1920, height: 1080, durationMs: 12_000 })!;
    expect(plan).toMatchObject({ width: 1280, height: 720 });
    expect(plan.videoKbps).toBeGreaterThan(5000);
    expect(((plan.videoKbps + 96) * 12) / 8 / 1024).toBeLessThan(8 * 1024 * 0.96);
  });

  it('a very short clip stops at a bitrate past which a 720p copy looks no better', () => {
    expect(embedPlan({ width: 1920, height: 1080, durationMs: 3000 })).toMatchObject({ videoKbps: 6000 });
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

  it('short copies are made while the uploader waits', () => {
    expect(embedPlan({ width: 1920, height: 1080, durationMs: 60_000 })).toMatchObject({ background: false, maxBytes: 10 * 1024 * 1024 });
  });

  it('a clip too long for a decent 8 MB copy gets a bigger one, made in the background', () => {
    const plan = embedPlan({ width: 1920, height: 1080, durationMs: 120_000 })!;
    expect(plan).toMatchObject({ background: true, width: 1280, height: 720, videoKbps: 2500 });
    const tenMinutes = embedPlan({ width: 1920, height: 1080, durationMs: 10 * 60_000 })!;
    expect(tenMinutes).toMatchObject({ background: true, width: 960, height: 540 });
    expect(((tenMinutes.videoKbps + 96) * 600) / 8 / 1024).toBeLessThan(50 * 1024 * 0.96);
  });

  it('a clip too long even for a 50 MB copy has none', () => {
    expect(embedPlan({ width: 1920, height: 1080, durationMs: 15 * 60_000 })).toBeNull();
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
    const file = join(clipsDir(), embedFileName(id));
    expect(existsSync(file)).toBe(true);
    const info = await probeVideo(file);
    expect(info).toMatchObject({ videoCodec: 'h264', audioCodec: 'aac', width: 1280, height: 720 });
    expect(info.fps).toBe(30);
    expect(Math.abs(info.durationMs - 2000)).toBeLessThan(150);
    const bytes = await readFile(file);
    expect(bytes.indexOf('moov')).toBeLessThan(bytes.indexOf('mdat'));

    const page = await t.app.inject({ url: `/c/${id}` });
    expect(page.body).toContain(`<meta property="og:video" content="http://localhost/media/${embedFileName(id)}"`);
    expect(page.body).toContain('<meta property="og:video:width" content="1280"');
    // The page's own player keeps the full video.
    expect(page.body).toContain(`src="http://localhost/media/${id}.mp4"`);

    const media = await t.app.inject({ url: `/media/${embedFileName(id)}` });
    expect(media.statusCode).toBe(200);
    expect(media.headers['content-type']).toBe('video/mp4');
  });

  it('without a copy the share page embeds the full video', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    await rm(join(clipsDir(), embedFileName(id)));
    const page = await t.app.inject({ url: `/c/${id}` });
    expect(page.body).toContain(`<meta property="og:video" content="http://localhost/media/${id}.mp4"`);
    // A missing copy is a 404, never the full video under its name.
    expect((await t.app.inject({ url: `/media/${embedFileName(id)}` })).statusCode).toBe(404);
  });

  it('deleting a clip removes its embed copy', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    const del = await t.app.inject({ method: 'DELETE', url: `/api/clips/${id}`, headers: aliceWeb });
    expect(del.statusCode).toBe(204);
    expect(existsSync(join(clipsDir(), embedFileName(id)))).toBe(false);
  });

  it('the backfill makes copies for clips that have none', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    await rm(join(clipsDir(), embedFileName(id)));
    const backfill = new EmbedBackfill({ db: t.db, clipsDir: clipsDir(), threads: 2, log: t.app.log });
    backfill.start();
    await backfill.idle();
    expect(existsSync(join(clipsDir(), embedFileName(id)))).toBe(true);
  });

  const embedVersionOf = async (id: string) =>
    (await t.db.select({ v: clips.embedVersion }).from(clips).where(eq(clips.id, id)))[0]!.v;
  const runBackfill = async () => {
    const backfill = new EmbedBackfill({ db: t.db, clipsDir: clipsDir(), threads: 2, log: t.app.log });
    backfill.start();
    await backfill.idle();
  };

  it('an upload records which embed settings made its copy', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    expect(await embedVersionOf(id)).toBe(EMBED_VERSION);
  });

  it('the backfill remakes a copy made by older embed settings, serving the old one meanwhile', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    const file = join(clipsDir(), embedFileName(id));
    await writeFile(file, 'old copy');
    await t.db.update(clips).set({ embedVersion: 0 }).where(eq(clips.id, id));
    // Until the new copy is made, the share page keeps embedding the old one, never the full video.
    const page = await t.app.inject({ url: `/c/${id}` });
    expect(page.body).toContain(`<meta property="og:video" content="http://localhost/media/${embedFileName(id)}"`);

    await runBackfill();
    expect(await probeVideo(file)).toMatchObject({ videoCodec: 'h264', width: 640, height: 360 });
    expect(await embedVersionOf(id)).toBe(EMBED_VERSION);
  });

  it('the backfill leaves copies made by the current settings alone', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    const file = join(clipsDir(), embedFileName(id));
    await writeFile(file, 'current copy');
    await runBackfill();
    expect(await readFile(file, 'utf8')).toBe('current copy');
  });

  it('a copy is cached briefly, since a settings change replaces it under the same URL', async () => {
    const { id } = await upload(await makeVideo(dir, '640x360', 30));
    const res = await t.app.inject({ url: `/media/${embedFileName(id)}` });
    expect(res.headers['cache-control']).toBe('public, max-age=3600');
    // The full video never changes under its name.
    expect((await t.app.inject({ url: `/media/${id}.mp4` })).headers['cache-control']).toContain('immutable');
  });
});

describe('shareReadyOf', () => {
  const clip = { kind: 'clip' as const, transcodeState: null, embedVersion: 0, embedPending: false };
  it('waits for a long clip\'s copy', () => {
    expect(shareReadyOf({ ...clip, embedPending: true })).toBe(false);
    expect(shareReadyOf({ ...clip, embedVersion: EMBED_VERSION })).toBe(true);
  });
  it('a clip with a copy is ready while it is compressed', () => {
    expect(shareReadyOf({ ...clip, transcodeState: 'running', embedVersion: EMBED_VERSION })).toBe(true);
  });
  it('a clip without a copy waits for compression, which makes its video small enough to embed', () => {
    expect(shareReadyOf({ ...clip, transcodeState: 'queued' })).toBe(false);
    expect(shareReadyOf({ ...clip, transcodeState: 'done' })).toBe(true);
    expect(shareReadyOf({ ...clip, transcodeState: 'failed' })).toBe(true);
  });
  it('screenshots are always ready', () => {
    expect(shareReadyOf({ ...clip, kind: 'screenshot', embedPending: true })).toBe(true);
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
    expect(existsSync(join(t.dataDir, 'clips', embedFileName(id)))).toBe(true);
  });

  it('a long upload waits for its embed copy, made in the background, before its link is ready', async () => {
    const alice = await createUser(t.db, { username: 'bob' });
    const token = (await sessionFor(t.db, alice.id, 'desktop')).token;
    // Too long for an 8 MB copy; tiny, so it encodes fast.
    const { uploadId } = await tusUpload(t.app, token, await readFile(await makeVideo(dir, '160x90', 10, 120)));
    const res = await t.app.inject({
      method: 'POST',
      url: '/api/clips',
      headers: { authorization: `Bearer ${token}` },
      payload: { uploadId, title: 'Long' },
    });
    expect(res.statusCode).toBe(201);
    const { id, shareReady } = res.json() as { id: string; shareReady: boolean };
    expect(shareReady).toBe(false);
    await t.app.embedBackfill!.idle();
    expect(existsSync(join(t.dataDir, 'clips', embedFileName(id)))).toBe(true);
    const get = await t.app.inject({ url: `/api/clips/${id}`, headers: { authorization: `Bearer ${token}` } });
    expect((get.json() as { shareReady: boolean }).shareReady).toBe(true);
    const page = await t.app.inject({ url: `/c/${id}` });
    expect(page.body).toContain(`<meta property="og:video" content="http://localhost/media/${embedFileName(id)}`);
  }, 60_000);
});
