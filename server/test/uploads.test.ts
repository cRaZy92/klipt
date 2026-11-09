import { existsSync } from 'node:fs';
import { mkdtemp, readdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { count, eq } from 'drizzle-orm';
import { clips, uploads } from '../src/db/schema.js';
import { cleanupStaleUploads } from '../src/uploads/tus.js';
import { createUser, makeFixtureVideo, listenOrigin, makeTestApp, sessionFor, tusUpload, type TestApp } from './helpers.js';

let t: TestApp;
let token: string;
let headers: Record<string, string>;
let fixture: Buffer;

beforeEach(async () => {
  t = await makeTestApp();
  const user = await createUser(t.db, { username: 'alice' });
  const s = await sessionFor(t.db, user.id, 'desktop');
  token = s.token;
  headers = s.headers;
  const dir = await mkdtemp(join(tmpdir(), 'klipt-fx-'));
  fixture = await readFile(await makeFixtureVideo(dir));
});
afterEach(() => t.close());

const finalize = (uploadId: string, extra: Record<string, unknown> = {}, h = headers) =>
  t.app.inject({ method: 'POST', url: '/api/clips', headers: h, payload: { uploadId, title: 'Nice', ...extra } });
const tusFiles = () => readdir(join(t.dataDir, 'tus')).catch(() => [] as string[]);

describe('uploads and finalize', () => {
  it('uploads fully then finalizes into a clip', async () => {
    const { uploadId, location } = await tusUpload(t.app, token, fixture, { chunkSize: 10_000 });
    expect(location).toBe(`http://localhost/api/uploads/${uploadId}`);
    const res = await finalize(uploadId, { gameName: 'Doom', steamAppId: 42 });
    expect(res.statusCode).toBe(201);
    const clip = res.json();
    expect(clip.shareUrl).toMatch(/^http:\/\/localhost\/c\/[A-Za-z0-9_-]{12}$/);
    expect(clip).toMatchObject({ title: 'Nice', gameName: 'Doom', steamAppId: 42, visibility: 'feed', owner: { username: 'alice' } });
    expect(clip.videoUrl).toBe(`http://localhost/media/${clip.id}.mp4`);
    expect(existsSync(join(t.dataDir, 'clips', `${clip.id}.mp4`))).toBe(true);
    expect(existsSync(join(t.dataDir, 'clips', `${clip.id}.jpg`))).toBe(true);
    expect(await tusFiles()).toEqual([]);
    expect(await t.db.select().from(uploads)).toHaveLength(0);
  });

  it('rejects unauthenticated uploads with 401', async () => {
    const res = await t.app.inject({
      method: 'POST',
      url: '/api/uploads',
      headers: { 'tus-resumable': '1.0.0', 'upload-length': '10' },
    });
    expect(res.statusCode).toBe(401);
  });

  it('resumes: HEAD reports the offset, remaining chunks complete the upload', async () => {
    const chunkSize = Math.ceil(fixture.length / 4);
    const { uploadId, location } = await tusUpload(t.app, token, fixture, { chunkSize, stopAfterChunks: 2 });
    const url = `${await listenOrigin(t.app)}${new URL(location).pathname}`;
    const head = await fetch(url, {
      method: 'HEAD',
      headers: { authorization: `Bearer ${token}`, 'tus-resumable': '1.0.0' },
    });
    expect(head.status).toBe(200);
    expect(head.headers.get('upload-offset')).toBe(String(2 * chunkSize));

    expect((await finalize(uploadId)).statusCode).toBe(409);

    const rest = fixture.subarray(2 * chunkSize);
    const patch = await fetch(url, {
      method: 'PATCH',
      headers: {
        authorization: `Bearer ${token}`,
        'tus-resumable': '1.0.0',
        'content-type': 'application/offset+octet-stream',
        'upload-offset': String(2 * chunkSize),
      },
      body: new Uint8Array(rest),
    });
    expect(patch.status).toBe(204);
    expect((await finalize(uploadId)).statusCode).toBe(201);
  });

  it('answers 409 upload_incomplete for partial uploads', async () => {
    const { uploadId } = await tusUpload(t.app, token, fixture, { chunkSize: 1000, stopAfterChunks: 1 });
    const res = await finalize(uploadId);
    expect(res.statusCode).toBe(409);
    expect(res.json().error).toBe('upload_incomplete');
  });

  it("answers 404 when finalizing another user's upload, and hides it from HEAD", async () => {
    const { uploadId, location } = await tusUpload(t.app, token, fixture);
    const bob = await createUser(t.db, { username: 'bob' });
    const b = await sessionFor(t.db, bob.id, 'desktop');
    expect((await finalize(uploadId, {}, b.headers)).statusCode).toBe(404);
    const head = await fetch(`${await listenOrigin(t.app)}${new URL(location).pathname}`, {
      method: 'HEAD',
      headers: { authorization: `Bearer ${b.token}`, 'tus-resumable': '1.0.0' },
    });
    expect(head.status).toBe(404);
  });

  it('rejects a non-video with 422 and cleans up', async () => {
    const { uploadId } = await tusUpload(t.app, token, Buffer.from('hello, I am not a video '.repeat(50)));
    const res = await finalize(uploadId);
    expect(res.statusCode).toBe(422);
    expect(res.json()).toMatchObject({ error: 'not_a_video', message: "That file isn't a video Klipt can play." });
    expect(await tusFiles()).toEqual([]);
    const [{ n }] = await t.db.select({ n: count() }).from(clips);
    expect(n).toBe(0);
    expect(await t.db.select().from(uploads)).toHaveLength(0);
    expect(await readdir(join(t.dataDir, 'clips'))).toEqual([]);
  });

  it('rejects uploads larger than the max size with 413', async () => {
    await t.close();
    t = await makeTestApp({ maxUploadMb: 1 });
    const user = await createUser(t.db, { username: 'alice' });
    const s = await sessionFor(t.db, user.id, 'desktop');
    const res = await t.app.inject({
      method: 'POST',
      url: '/api/uploads',
      headers: { authorization: `Bearer ${s.token}`, 'tus-resumable': '1.0.0', 'upload-length': String(2 * 1024 * 1024) },
    });
    expect(res.statusCode).toBe(413);
  });

  it('cleans up stale uploads only after the max age', async () => {
    const { uploadId } = await tusUpload(t.app, token, fixture, { chunkSize: 1000, stopAfterChunks: 1 });
    const [row] = await t.db.select().from(uploads).where(eq(uploads.id, uploadId));
    const h = (n: number) => new Date(row!.createdAt.getTime() + n * 3600_000);
    expect(await cleanupStaleUploads(t.db, t.dataDir, h(1))).toBe(0);
    expect(existsSync(join(t.dataDir, 'tus', uploadId))).toBe(true);
    expect(await cleanupStaleUploads(t.db, t.dataDir, h(25))).toBe(1);
    expect(await tusFiles()).toEqual([]);
    expect(await t.db.select().from(uploads)).toHaveLength(0);
  });

  it('validates the title', async () => {
    const { uploadId } = await tusUpload(t.app, token, fixture);
    expect((await finalize(uploadId, { title: '   ' })).statusCode).toBe(400);
    expect((await finalize(uploadId, { title: 'x'.repeat(101) })).statusCode).toBe(400);
  });
});
