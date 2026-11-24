import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { eq } from 'drizzle-orm';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { clips } from '../src/db/schema.js';
import { DiskGuard, freeBytes } from '../src/disk.js';
import { createUser, listenOrigin, makeFixtureVideo, makeTestApp, sessionFor, tusUpload, type TestApp } from './helpers.js';

/** More than any test machine has free. */
const PETABYTE = 1024 ** 5;

describe('DiskGuard', () => {
  let dir: string;
  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'klipt-disk-'));
  });
  afterEach(() => rm(dir, { recursive: true, force: true }));

  it('reads free space, and refuses work that would go below the margin', async () => {
    expect(await freeBytes(dir)).toBeGreaterThan(0);
    expect(await new DiskGuard(dir, 0).roomFor(1024)).toBe(true);
    expect(await new DiskGuard(dir, 0).roomFor(PETABYTE)).toBe(false);
    expect(await new DiskGuard(dir, PETABYTE).roomFor(0)).toBe(false);
    expect(await new DiskGuard(dir, 0).belowFloor()).toBe(false);
    expect(await new DiskGuard(dir, PETABYTE).belowFloor()).toBe(true);
    expect(await new DiskGuard(dir, PETABYTE).status()).toMatchObject({ low: true });
    expect(await new DiskGuard(dir, 0).status()).toMatchObject({ low: false });
  });

  it('refuses nothing when free space cannot be read', async () => {
    const gone = new DiskGuard(join(dir, 'missing'), PETABYTE);
    expect(await gone.roomFor(PETABYTE)).toBe(true);
    expect(await gone.belowFloor()).toBe(false);
    expect(await gone.status()).toEqual({ freeBytes: null, low: false });
  });
});

describe('uploads on a nearly full disk', () => {
  let t: TestApp;
  let token: string;
  let fixture: Buffer;

  beforeEach(async () => {
    t = await makeTestApp();
    const user = await createUser(t.db, { username: 'alice' });
    token = (await sessionFor(t.db, user.id, 'desktop')).token;
    fixture = await readFile(await makeFixtureVideo(await mkdtemp(join(tmpdir(), 'klipt-fx-'))));
  });
  afterEach(() => t.close());

  const auth = () => ({ authorization: `Bearer ${token}`, 'tus-resumable': '1.0.0' });
  const create = async (length: number) =>
    fetch(`${await listenOrigin(t.app)}/api/uploads`, { method: 'POST', headers: { ...auth(), 'upload-length': String(length) } });

  it('refuses a new upload with 507 when it would leave too little free', async () => {
    t.app.disk.minFreeBytes = PETABYTE;
    expect((await create(fixture.length)).status).toBe(507);
    t.app.disk.minFreeBytes = 0;
    expect((await create(fixture.length)).status).toBe(201);
  });

  it("counts the upload's own size", async () => {
    // The margin alone fits; the margin plus 50 MB doesn't.
    const free = (await freeBytes(t.dataDir))!;
    t.app.disk.minFreeBytes = free - 1024 * 1024;
    expect((await create(50 * 1024 * 1024)).status).toBe(507);
  });

  it('stops taking bytes of an accepted upload below the floor; it carries on once there is room', async () => {
    const created = await create(fixture.length);
    expect(created.status).toBe(201);
    const url = `${await listenOrigin(t.app)}${new URL(created.headers.get('location')!).pathname}`;
    const patch = () =>
      fetch(url, {
        method: 'PATCH',
        headers: { ...auth(), 'upload-offset': '0', 'content-type': 'application/offset+octet-stream' },
        body: new Uint8Array(fixture),
      });
    t.app.disk.minFreeBytes = PETABYTE;
    expect((await patch()).status).toBe(507);
    t.app.disk.minFreeBytes = 0;
    expect((await patch()).status).toBe(204);
  });

  it('shows free space on the admin page', async () => {
    const adm = await createUser(t.db, { username: 'boss', isAdmin: true });
    const headers = (await sessionFor(t.db, adm.id)).headers;
    const stats = (await t.app.inject({ url: '/api/admin/stats', headers })).json();
    expect(stats.freeBytes).toBeGreaterThan(0);
    expect(stats.diskLow).toBe(false);
    t.app.disk.minFreeBytes = PETABYTE;
    expect((await t.app.inject({ url: '/api/admin/stats', headers })).json().diskLow).toBe(true);
    const page = await t.app.inject({ url: '/admin', headers });
    expect(page.body).toContain('free, uploads wait');
  });
});

describe('compression on a nearly full disk', () => {
  let t: TestApp;
  afterEach(() => t.close());

  it('leaves the clip queued until there is room', async () => {
    t = await makeTestApp({ transcode: true });
    await t.app.ready();
    const alice = await createUser(t.db, { username: 'alice' });
    const token = (await sessionFor(t.db, alice.id, 'desktop')).token;
    const fixture = await readFile(await makeFixtureVideo(await mkdtemp(join(tmpdir(), 'klipt-fx-'))));
    const { uploadId } = await tusUpload(t.app, token, fixture);

    t.app.disk.minFreeBytes = PETABYTE;
    const res = await t.app.inject({
      method: 'POST',
      url: '/api/clips',
      headers: { authorization: `Bearer ${token}` },
      payload: { uploadId, title: 'Ace' },
    });
    expect(res.statusCode).toBe(201);
    const id = res.json().id as string;
    await t.app.transcoder.idle();
    const state = async () => (await t.db.select({ s: clips.transcodeState }).from(clips).where(eq(clips.id, id)))[0]!.s;
    expect(await state()).toBe('queued');

    t.app.disk.minFreeBytes = 0;
    t.app.transcoder.kick();
    await t.app.transcoder.idle();
    expect(['done', 'skipped']).toContain(await state());
  });
});
