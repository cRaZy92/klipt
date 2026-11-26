import { existsSync } from 'node:fs';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { clips } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
let a: { id: string; h: Record<string, string> };
let b: { id: string; h: Record<string, string> };

async function mk(name: string) {
  const u = await createUser(t.db, { username: name });
  return { id: u.id, h: (await sessionFor(t.db, u.id)).headers };
}

/** A picture made by ffmpeg, as `ext`. */
async function picture(ext: string, size = '640x400'): Promise<{ buf: Buffer; path: string }> {
  const dir = await mkdtemp(join(tmpdir(), 'klipt-avatar-'));
  const path = join(dir, `p.${ext}`);
  execFileSync(process.env.FFMPEG_PATH ?? 'ffmpeg', ['-v', 'error', '-y', '-f', 'lavfi', '-i', `color=blue:size=${size}`, '-frames:v', '1', path]);
  return { buf: await readFile(path), path };
}

const put = (h: Record<string, string>, body: Buffer | string, type = 'image/png') =>
  t.app.inject({ method: 'PUT', url: '/api/me/avatar', headers: { ...h, 'content-type': type }, payload: body });

beforeEach(async () => {
  t = await makeTestApp();
  a = await mk('alice');
  b = await mk('bob');
});
afterEach(() => t.close());

describe('profile pictures', () => {
  it('stores a 256×256 JPEG and serves it as immutable', async () => {
    const res = await put(a.h, (await picture('png')).buf);
    expect(res.statusCode).toBe(200);
    expect(res.json().avatarUrl).toBe(`http://localhost/avatars/${a.id}.jpg?v=1`);
    const img = await t.app.inject({ method: 'GET', url: `/avatars/${a.id}.jpg?v=1` });
    expect(img.statusCode).toBe(200);
    expect(img.headers['content-type']).toBe('image/jpeg');
    expect(img.headers['cache-control']).toBe('public, max-age=31536000, immutable');
    const out = join(t.dataDir, 'avatars', `${a.id}.jpg`);
    const probe = execFileSync(process.env.FFPROBE_PATH ?? 'ffprobe', ['-v', 'error', '-show_entries', 'stream=width,height', '-of', 'csv=p=0', out]).toString().trim();
    expect(probe).toBe('256,256');
    const again = await put(a.h, (await picture('webp')).buf, 'image/webp');
    expect(again.json().avatarUrl).toMatch(/\?v=2$/);
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: a.h })).json().avatarUrl).toMatch(/\?v=2$/);
  });

  it('refuses what is not a picture and keeps the old one', async () => {
    await put(a.h, (await picture('jpg')).buf, 'image/jpeg');
    expect((await put(a.h, 'hello', 'text/plain')).statusCode).toBe(415);
    const bad = await put(a.h, Buffer.from('not an image at all'), 'image/png');
    expect(bad.statusCode).toBe(422);
    expect(bad.json().error).toBe('not_an_image');
    expect((await t.app.inject({ method: 'GET', url: '/api/me', headers: a.h })).json().avatarUrl).toMatch(/\?v=1$/);
    expect(existsSync(join(t.dataDir, 'avatars', `${a.id}.jpg`))).toBe(true);
  });

  it('needs a login', async () => {
    expect((await put({}, (await picture('png')).buf)).statusCode).toBe(401);
  });

  it('goes back to the initial when removed', async () => {
    await put(a.h, (await picture('png')).buf);
    const res = await t.app.inject({ method: 'DELETE', url: '/api/me/avatar', headers: a.h });
    expect(res.statusCode).toBe(200);
    expect(res.json().avatarUrl).toBeNull();
    expect(existsSync(join(t.dataDir, 'avatars', `${a.id}.jpg`))).toBe(false);
    expect((await t.app.inject({ method: 'GET', url: `/avatars/${a.id}.jpg` })).statusCode).toBe(404);
  });

  it('a picture set after removing one gets a new URL (the old one is cached for good)', async () => {
    await put(a.h, (await picture('png')).buf);
    await t.app.inject({ method: 'DELETE', url: '/api/me/avatar', headers: a.h });
    const again = await put(a.h, (await picture('jpg')).buf, 'image/jpeg');
    expect(again.json().avatarUrl).toBe(`http://localhost/avatars/${a.id}.jpg?v=2`);
  });

  it('serves only avatar names', async () => {
    await writeFile(join(t.dataDir, 'secret.jpg'), 'x');
    expect((await t.app.inject({ method: 'GET', url: '/avatars/..%2Fsecret.jpg' })).statusCode).toBe(404);
    expect((await t.app.inject({ method: 'GET', url: '/avatars/nope.jpg' })).statusCode).toBe(404);
  });

  it('shows on clips, the friends list and viewers', async () => {
    await put(a.h, (await picture('png')).buf);
    await t.db.insert(clips).values({ id: 'clipAAAAAAAA', userId: a.id, title: 'x', durationMs: 1000, width: 1, height: 1, sizeBytes: 1 });
    const list = (await t.app.inject({ method: 'GET', url: '/api/clips', headers: b.h })).json();
    expect(list.items[0].owner).toEqual({ username: 'alice', displayName: 'alice', avatarUrl: `http://localhost/avatars/${a.id}.jpg?v=1`, frame: null });
    const users = (await t.app.inject({ method: 'GET', url: '/api/users', headers: b.h })).json();
    expect(users.find((u: { username: string }) => u.username === 'alice').avatarUrl).toMatch(/\?v=1$/);
    expect(users.find((u: { username: string }) => u.username === 'bob').avatarUrl).toBeNull();
    await t.app.inject({ method: 'POST', url: '/api/clips/clipAAAAAAAA/view', headers: b.h });
    await put(b.h, (await picture('png')).buf);
    const viewers = (await t.app.inject({ method: 'GET', url: '/api/clips/clipAAAAAAAA/viewers', headers: a.h })).json();
    expect(viewers[0]).toMatchObject({ username: 'bob', avatarUrl: `http://localhost/avatars/${b.id}.jpg?v=1`, frame: null });
  });
});
