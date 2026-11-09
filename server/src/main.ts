import { loadConfig } from './config.js';
import { createDb, runMigrations } from './db/client.js';
import { ensureAdmin } from './admin/bootstrap.js';
import { buildApp } from './app.js';
import { cleanupStaleUploads } from './uploads/tus.js';

const HOUR_MS = 60 * 60 * 1000;

async function main(): Promise<void> {
  let config;
  try {
    config = loadConfig(process.env);
  } catch (err) {
    console.error(err instanceof Error ? err.message : err);
    process.exit(1);
  }

  const db = await createDb(config.databaseUrl);
  await runMigrations(db);
  await ensureAdmin(db, config);
  const app = await buildApp({ config, db });

  const cleanup = async () => {
    try {
      const n = await cleanupStaleUploads(db, config.dataDir, new Date());
      if (n > 0) app.log.info(`removed ${n} stale uploads`);
    } catch (err) {
      app.log.error({ err }, 'stale upload cleanup failed');
    }
  };
  await cleanup();
  const timer = setInterval(cleanup, HOUR_MS);

  const shutdown = (signal: string) => {
    console.log(`${signal} received, shutting down`);
    clearInterval(timer);
    app.close().then(
      () => process.exit(0),
      (err) => {
        console.error(err);
        process.exit(1);
      },
    );
  };
  process.on('SIGTERM', () => shutdown('SIGTERM'));
  process.on('SIGINT', () => shutdown('SIGINT'));

  await app.listen({ host: '0.0.0.0', port: config.port });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
