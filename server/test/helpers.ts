import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { FastifyInstance } from 'fastify';
import { buildApp } from '../src/app.js';
import { loadConfig, type Config } from '../src/config.js';
import { createTestDb, type Db } from '../src/db/client.js';

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
