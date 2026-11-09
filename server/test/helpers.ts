import { execFile } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { FastifyInstance } from 'fastify';
import { buildApp } from '../src/app.js';
import { loadConfig, type Config } from '../src/config.js';
import { createTestDb, type Db } from '../src/db/client.js';
import { users } from '../src/db/schema.js';
import { hashPassword } from '../src/auth/passwords.js';
import { createSession } from '../src/auth/sessions.js';

export interface TestApp {
  app: FastifyInstance;
  db: Db;
  config: Config;
  dataDir: string;
  close(): Promise<void>;
}

export async function makeTestApp(opts: { maxUploadMb?: number } = {}): Promise<TestApp> {
  const dataDir = await mkdtemp(join(tmpdir(), 'klipt-test-'));
  const env: Record<string, string> = {
    BASE_URL: 'http://localhost',
    DATABASE_URL: 'pglite://memory',
    DATA_DIR: dataDir,
  };
  if (opts.maxUploadMb !== undefined) env.MAX_UPLOAD_MB = String(opts.maxUploadMb);
  const config = loadConfig(env);
  const db = await createTestDb();
  const app = await buildApp({ config, db });
  return {
    app,
    db,
    config,
    dataDir,
    async close() {
      await app.close();
      await rm(dataDir, { recursive: true, force: true });
    },
  };
}

export async function createUser(
  db: Db,
  opts: { username: string; password?: string; isAdmin?: boolean },
) {
  const [user] = await db
    .insert(users)
    .values({
      username: opts.username,
      usernameKey: opts.username.toLowerCase(),
      passwordHash: await hashPassword(opts.password ?? 'password123'),
      isAdmin: opts.isAdmin ?? false,
    })
    .returning();
  return user!;
}

/** Creates a session directly (no login, so no rate limit). */
export async function sessionFor(db: Db, userId: string, kind: 'web' | 'desktop' = 'web') {
  const { token, session } = await createSession(db, userId, kind);
  const headers: Record<string, string> =
    kind === 'web' ? { cookie: `klipt_session=${token}` } : { authorization: `Bearer ${token}` };
  return { token, session, headers, cookie: `klipt_session=${token}` };
}

/** Logs in through the API. Returns the cookie header (web) or the bearer token (desktop). */
export async function loginAs(
  app: FastifyInstance,
  username: string,
  password: string,
  client: 'web' | 'desktop' = 'web',
): Promise<string> {
  const res = await app.inject({
    method: 'POST',
    url: '/api/auth/login',
    payload: { username, password, client },
  });
  if (res.statusCode !== 200) throw new Error(`login failed: ${res.statusCode} ${res.body}`);
  if (client === 'desktop') return (res.json() as { token: string }).token;
  const c = res.cookies.find((x) => x.name === 'klipt_session');
  if (!c) throw new Error('no session cookie');
  return `klipt_session=${c.value}`;
}

/** Generates a small deterministic MP4 (no faststart) in `dir` and returns its path. */
export async function makeFixtureVideo(
  dir: string,
  opts: { seconds?: number; codec?: 'libx264' | 'mpeg4'; audio?: boolean } = {},
): Promise<string> {
  const { seconds = 2, codec = 'libx264', audio = true } = opts;
  const out = join(dir, `fixture-${codec}-${audio ? 'a' : 'na'}-${seconds}.mp4`);
  const args = ['-v', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc=size=320x240:rate=30'];
  if (audio) args.push('-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=44100');
  args.push('-t', String(seconds), '-c:v', codec, '-pix_fmt', 'yuv420p');
  if (codec === 'libx264') args.push('-preset', 'ultrafast');
  if (audio) args.push('-c:a', 'aac');
  args.push(out);
  await new Promise<void>((resolve, reject) => {
    execFile(process.env.FFMPEG_PATH ?? 'ffmpeg', args, (err, _o, stderr) =>
      err ? reject(new Error(`ffmpeg failed: ${stderr || err.message}`)) : resolve(),
    );
  });
  return out;
}

/**
 * Starts the app on an ephemeral loopback port (once) and returns its origin.
 * @tus/server speaks raw node req/res; app.inject hangs on streamed PATCH bodies, so tus tests use real HTTP.
 */
export async function listenOrigin(app: FastifyInstance): Promise<string> {
  if (!app.server.listening) await app.listen({ port: 0, host: '127.0.0.1' });
  const addr = app.server.address();
  if (!addr || typeof addr === 'string') throw new Error('app is not listening on a TCP port');
  return `http://127.0.0.1:${addr.port}`;
}

/** Uploads `bytes` over the tus 1.0 protocol (POST create, then PATCH chunks) against a real listening server. */
export async function tusUpload(
  app: FastifyInstance,
  token: string,
  bytes: Buffer,
  opts: { chunkSize?: number; stopAfterChunks?: number } = {},
): Promise<{ uploadId: string; location: string }> {
  const { chunkSize = 256 * 1024, stopAfterChunks = Infinity } = opts;
  const origin = await listenOrigin(app);
  const auth = { authorization: `Bearer ${token}`, 'tus-resumable': '1.0.0' };
  const created = await fetch(`${origin}/api/uploads`, {
    method: 'POST',
    headers: { ...auth, 'upload-length': String(bytes.length) },
  });
  if (created.status !== 201) throw new Error(`tus create failed: ${created.status} ${await created.text()}`);
  const location = created.headers.get('location')!;
  const url = `${origin}${new URL(location).pathname}`;
  const uploadId = url.split('/').pop()!;
  let offset = 0;
  for (let i = 0; i < stopAfterChunks && offset < bytes.length; i++) {
    const chunk = bytes.subarray(offset, offset + chunkSize);
    const res = await fetch(url, {
      method: 'PATCH',
      headers: { ...auth, 'content-type': 'application/offset+octet-stream', 'upload-offset': String(offset) },
      body: new Uint8Array(chunk),
    });
    if (res.status !== 204) throw new Error(`tus patch failed: ${res.status} ${await res.text()}`);
    offset += chunk.length;
  }
  return { uploadId, location };
}
