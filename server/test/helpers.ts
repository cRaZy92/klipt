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
