import { loadConfig } from './config.js';
import { createDb, runMigrations } from './db/client.js';
import { ensureAdmin } from './admin/bootstrap.js';
import { buildApp } from './app.js';
import { cleanupStaleUploads } from './uploads/tus.js';
import { cleanupExpired } from './cleanup.js';
import { evaluateAll } from './achievements/engine.js';

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
  const app = await buildApp({ config, db, logger: true });
  // Achievements for what was already there (first deploy, or definitions added since): awarded quietly.
  try {
    const n = await evaluateAll(db);
    if (n > 0) app.log.info(`awarded ${n} achievements for past activity`);
  } catch (err) {
    app.log.error({ err }, 'checking achievements for past activity failed');
  }

  const cleanup = async () => {
    try {
      const n = await cleanupStaleUploads(db, config.dataDir, new Date());
      if (n > 0) app.log.info(`removed ${n} stale uploads`);
    } catch (err) {
      app.log.error({ err }, 'stale upload cleanup failed');
    }
    try {
      const n = await cleanupExpired(db, new Date());
      if (n.sessions + n.passwordResets + n.invites > 0) app.log.info(n, 'removed expired logins, password links and invites');
    } catch (err) {
      app.log.error({ err }, 'expired record cleanup failed');
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
