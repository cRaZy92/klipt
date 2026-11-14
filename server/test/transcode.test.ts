import { execFile } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { eq } from 'drizzle-orm';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { clips } from '../src/db/schema.js';
import { probeVideo, transcodeH264 } from '../src/clips/media.js';
import { createUser, makeTestApp, sessionFor, tusUpload, type TestApp } from './helpers.js';

/** A 2 s 640x360 H.264 clip: `quality` 'high' is a big near-lossless file (compresses well), 'low' a small one (doesn't). */
async function makeVideo(dir: string, quality: 'high' | 'low'): Promise<string> {
  const out = join(dir, `src-${quality}.mp4`);
  const crf = quality === 'high' ? '4' : '40';
  const args = ['-v', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=30', '-f', 'lavfi', '-i', 'sine=frequency=440'];
  args.push('-t', '2', '-c:v', 'libx264', '-preset', 'ultrafast', '-crf', crf, '-pix_fmt', 'yuv420p', '-c:a', 'aac', out);
  await new Promise<void>((resolve, reject) =>
    execFile(process.env.FFMPEG_PATH ?? 'ffmpeg', args, (err, _o, stderr) => (err ? reject(new Error(stderr || err.message)) : resolve())),
  );
  return out;
}

let dir: string;
beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), 'klipt-tc-'));
});
afterEach(async () => {
  await rm(dir, { recursive: true, force: true });
});

describe('transcodeH264', () => {
  it('re-encodes to a smaller H.264 file with the audio, reporting progress', async () => {
    const src = await makeVideo(dir, 'high');
    const out = join(dir, 'out.mp4');
    const progress: number[] = [];
    await transcodeH264(src, out, { crf: 23, preset: 'veryfast', threads: 2, durationMs: 2000, onProgress: (p) => progress.push(p) });
    const info = await probeVideo(out);
    expect(info).toMatchObject({ videoCodec: 'h264', audioCodec: 'aac', width: 640, height: 360 });
    expect(Math.abs(info.durationMs - 2000)).toBeLessThan(150);
    expect((await readFile(out)).length).toBeLessThan((await readFile(src)).length);
    expect(progress.length).toBeGreaterThan(0);
    expect(Math.max(...progress)).toBeGreaterThan(0.5);
    const bytes = await readFile(out);
    expect(bytes.indexOf('moov')).toBeLessThan(bytes.indexOf('mdat'));
  });

  it('rejects a file that is not a video', async () => {
    const bad = join(dir, 'bad.mp4');
    await writeFile(bad, 'nope');
    await expect(transcodeH264(bad, join(dir, 'o.mp4'), { crf: 23, preset: 'veryfast', threads: 1, durationMs: 1000 })).rejects.toMatchObject({
      code: 'corrupt_video',
    });
  });
});

describe('compression queue', () => {
  let t: TestApp;
  let token: string;
  let admin: Record<string, string>;
  let aliceWeb: Record<string, string>;

  beforeEach(async () => {
    t = await makeTestApp({ transcode: true });
    await t.app.ready();
    const alice = await createUser(t.db, { username: 'alice' });
    token = (await sessionFor(t.db, alice.id, 'desktop')).token;
    aliceWeb = (await sessionFor(t.db, alice.id)).headers;
    const adm = await createUser(t.db, { username: 'boss', isAdmin: true });
    admin = (await sessionFor(t.db, adm.id)).headers;
  });
  afterEach(() => t.close());

  const upload = async (file: string) => {
    const { uploadId } = await tusUpload(t.app, token, await readFile(file));
    const res = await t.app.inject({
      method: 'POST',
      url: '/api/clips',
      headers: { authorization: `Bearer ${token}` },
      payload: { uploadId, title: 'Ace' },
    });
    expect(res.statusCode).toBe(201);
    return res.json() as { id: string; processing: string | null; videoUrl: string; sizeBytes: number };
  };
  const getClip = async (id: string) => (await t.app.inject({ url: `/api/clips/${id}`, headers: aliceWeb })).json();

  it('compresses an upload in the background and swaps in the smaller file', async () => {
    const created = await upload(await makeVideo(dir, 'high'));
    expect(['queued', 'running']).toContain(created.processing);
    expect(created.videoUrl).toBe(`http://localhost/media/${created.id}.mp4`);

    await t.app.transcoder.idle();
    const clip = await getClip(created.id);
    expect(clip.processing).toBeNull();
    expect(clip.videoUrl).toBe(`http://localhost/media/${created.id}.v1.mp4`);
    expect(clip.sizeBytes).toBeLessThan(created.sizeBytes);

    const media = await t.app.inject({ url: `/media/${created.id}.v1.mp4` });
    expect(media.statusCode).toBe(200);
    expect(Number(media.headers['content-length'])).toBe(clip.sizeBytes);
    // The upload stays for a while for viewers who are mid-play.
    expect(existsSync(join(t.dataDir, 'clips', `${created.id}.mp4`))).toBe(true);

    const [row] = await t.db.select().from(clips).where(eq(clips.id, created.id));
    expect(row).toMatchObject({ transcodeState: 'done', mediaVersion: 1, originalBytes: created.sizeBytes });

    const q = (await t.app.inject({ url: '/api/admin/transcodes', headers: admin })).json();
    expect(q.counts).toMatchObject({ done: 1, queued: 0, running: 0 });
    expect(q.savedBytes).toBe(created.sizeBytes - clip.sizeBytes);
    expect(q.recent[0]).toMatchObject({ clipId: created.id, state: 'done', owner: 'alice', originalBytes: created.sizeBytes });

    const page = await t.app.inject({ url: '/admin', headers: admin });
    expect(page.statusCode).toBe(200);
    expect(page.body).toMatch(/→ .*\(−\d+%\)/);
  });

  it('keeps the upload when compressing barely helps', async () => {
    const created = await upload(await makeVideo(dir, 'low'));
    await t.app.transcoder.idle();
    const clip = await getClip(created.id);
    expect(clip.processing).toBeNull();
    expect(clip.videoUrl).toBe(`http://localhost/media/${created.id}.mp4`);
    const [row] = await t.db.select().from(clips).where(eq(clips.id, created.id));
    expect(row).toMatchObject({ transcodeState: 'skipped', mediaVersion: 0, sizeBytes: created.sizeBytes });
    expect(existsSync(join(t.dataDir, 'clips', `${created.id}.v1.mp4`))).toBe(false);
  });

  it('marks a broken file failed with the reason, and an admin can retry it', async () => {
    const created = await upload(await makeVideo(dir, 'high'));
    await t.app.transcoder.idle();
    // Simulate a clip whose stored file went bad, then queue it again.
    await t.db.update(clips).set({ transcodeState: 'queued', mediaVersion: 0 }).where(eq(clips.id, created.id));
    await writeFile(join(t.dataDir, 'clips', `${created.id}.mp4`), 'garbage');
    t.app.transcoder.kick();
    await t.app.transcoder.idle();

    const q = (await t.app.inject({ url: '/api/admin/transcodes', headers: admin })).json();
    expect(q.counts.failed).toBe(1);
    expect(q.recent[0]).toMatchObject({ clipId: created.id, state: 'failed' });
    expect(q.recent[0].error).toMatch(/ffmpeg failed/);

    const page = await t.app.inject({ url: '/admin', headers: admin });
    expect(page.body).toContain('Video compression');
    expect(page.body).toContain('data-action="retry-transcode"');

    const notAdmin = await t.app.inject({ method: 'POST', url: `/api/admin/transcodes/${created.id}/retry`, headers: aliceWeb });
    expect(notAdmin.statusCode).toBe(403);
    // Only a failed one can be retried; the next attempt fails again here, so stop the worker first.
    await t.app.transcoder.stop();
    const retry = await t.app.inject({ method: 'POST', url: `/api/admin/transcodes/${created.id}/retry`, headers: admin });
    expect(retry.statusCode).toBe(204);
    expect((await getClip(created.id)).processing).toBe('queued');
    const again = await t.app.inject({ method: 'POST', url: `/api/admin/transcodes/${created.id}/retry`, headers: admin });
    expect(again.statusCode).toBe(404);
  });

  it('shows a Queued pill on the feed while a clip waits', async () => {
    await t.app.transcoder.stop();
    const created = await upload(await makeVideo(dir, 'low'));
    expect(created.processing).toBe('queued');
    const feed = await t.app.inject({ url: '/', headers: aliceWeb });
    expect(feed.body).toContain('class="proc queued"');
    const me = await t.app.inject({ url: '/me', headers: aliceWeb });
    expect(me.body).toContain('class="proc queued"');
  });

  it('the live admin fragment is admin only', async () => {
    expect((await t.app.inject({ url: '/admin/transcodes', headers: aliceWeb })).statusCode).toBe(404);
    const res = await t.app.inject({ url: '/admin/transcodes', headers: admin });
    expect(res.statusCode).toBe(200);
    expect(res.body).toMatch(/^<section id="transcodes"/);
  });

  it('deleting a compressed clip removes both its files', async () => {
    const created = await upload(await makeVideo(dir, 'high'));
    await t.app.transcoder.idle();
    const del = await t.app.inject({ method: 'DELETE', url: `/api/clips/${created.id}`, headers: aliceWeb });
    expect(del.statusCode).toBe(204);
    for (const f of [`${created.id}.mp4`, `${created.id}.v1.mp4`, `${created.id}.jpg`]) {
      expect(existsSync(join(t.dataDir, 'clips', f))).toBe(false);
    }
  });

  it('media names: serves versioned videos, refuses made-up ones', async () => {
    const created = await upload(await makeVideo(dir, 'high'));
    await t.app.transcoder.idle();
    expect((await t.app.inject({ url: `/media/${created.id}.v0.mp4` })).statusCode).toBe(404);
    expect((await t.app.inject({ url: `/media/${created.id}.v1.jpg` })).statusCode).toBe(404);
    expect((await t.app.inject({ url: `/media/${created.id}.v2.mp4` })).statusCode).toBe(404);
    const dl = await t.app.inject({ url: `/media/${created.id}.v1.mp4?download=1` });
    expect(dl.headers['content-disposition']).toContain('Ace.mp4');
  });

  it('a link to the upload keeps working after the compressed file replaced it', async () => {
    // Discord caches a share page's og:video for the URL it saw when the link was posted,
    // which is the upload's name if the clip was still compressing.
    const created = await upload(await makeVideo(dir, 'high'));
    await t.app.transcoder.idle();
    await rm(join(t.dataDir, 'clips', `${created.id}.mp4`)); // what the grace timer does later
    const clip = await getClip(created.id);

    const old = await t.app.inject({ url: `/media/${created.id}.mp4` });
    expect(old.statusCode).toBe(200);
    expect(Number(old.headers['content-length'])).toBe(clip.sizeBytes);
    expect(old.headers['cache-control']).not.toContain('immutable');
    const range = await t.app.inject({ url: `/media/${created.id}.mp4`, headers: { range: 'bytes=0-99' } });
    expect(range.statusCode).toBe(206);
    expect((await t.app.inject({ url: `/media/${created.id}.v2.mp4` })).statusCode).toBe(404);
  });
});

describe('restart recovery', () => {
  it('requeues an interrupted job and removes files no clip points at', async () => {
    const t = await makeTestApp({ transcode: true });
    try {
      const user = await createUser(t.db, { username: 'alice' });
      const clipsDir = join(t.dataDir, 'clips');
      const src = await makeVideo(dir, 'high');
      await writeFile(join(clipsDir, 'AAAAAAAAAAAA.mp4'), await readFile(src));
      // A result from a job that died before its database write, and a half-written temp file.
      await writeFile(join(clipsDir, 'AAAAAAAAAAAA.v1.mp4'), 'stale');
      await writeFile(join(clipsDir, '.AAAAAAAAAAAA.v1.mp4.tmp'), 'partial');
      // A file of an upload that isn't a clip (yet): left alone.
      await writeFile(join(clipsDir, 'BBBBBBBBBBBB.mp4'), 'x');
      await t.db.insert(clips).values({
        id: 'AAAAAAAAAAAA',
        userId: user.id,
        title: 'Old',
        durationMs: 2000,
        width: 640,
        height: 360,
        sizeBytes: (await readFile(src)).length,
        transcodeState: 'running',
      });
      await t.app.ready();
      expect(existsSync(join(clipsDir, '.AAAAAAAAAAAA.v1.mp4.tmp'))).toBe(false);
      expect(existsSync(join(clipsDir, 'BBBBBBBBBBBB.mp4'))).toBe(true);
      await t.app.transcoder.idle();
      const [row] = await t.db.select().from(clips).where(eq(clips.id, 'AAAAAAAAAAAA'));
      expect(row).toMatchObject({ transcodeState: 'done', mediaVersion: 1 });
      expect((await readFile(join(clipsDir, 'AAAAAAAAAAAA.v1.mp4'))).length).toBe(row!.sizeBytes);
    } finally {
      await t.close();
    }
  });

  it('with compression off, queued clips stop showing as waiting', async () => {
    const t = await makeTestApp();
    try {
      const user = await createUser(t.db, { username: 'alice' });
      await t.db.insert(clips).values({ id: 'CCCCCCCCCCCC', userId: user.id, title: 'x', durationMs: 1, width: 1, height: 1, sizeBytes: 1, transcodeState: 'queued' });
      await t.app.ready();
      const [row] = await t.db.select().from(clips).where(eq(clips.id, 'CCCCCCCCCCCC'));
      expect(row!.transcodeState).toBeNull();
    } finally {
      await t.close();
    }
  });
});
