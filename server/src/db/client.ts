import { fileURLToPath } from 'node:url';
import { drizzle as drizzlePg } from 'drizzle-orm/node-postgres';
import { drizzle as drizzlePglite } from 'drizzle-orm/pglite';
import { readMigrationFiles } from 'drizzle-orm/migrator';
import type { PgDatabase, PgQueryResultHKT } from 'drizzle-orm/pg-core';
import { PGlite } from '@electric-sql/pglite';
import pg from 'pg';
import * as schema from './schema.js';

/** Common drizzle type shared by node-postgres (prod) and PGlite (tests). */
export type Db = PgDatabase<PgQueryResultHKT, typeof schema>;

const defaultMigrationsDir = fileURLToPath(new URL('./migrations', import.meta.url));

export async function runMigrations(
  db: Db,
  folder: string = process.env.MIGRATIONS_DIR ?? defaultMigrationsDir,
): Promise<void> {
  const migrations = readMigrationFiles({ migrationsFolder: folder });
  // Same call both drizzle migrators make; avoids driver-specific migrator imports.
  const internals = db as unknown as {
    dialect: { migrate(m: typeof migrations, session: unknown, cfg: object): Promise<void> };
    session: unknown;
  };
  await internals.dialect.migrate(migrations, internals.session, { migrationsFolder: folder });
}

export async function createDb(databaseUrl: string): Promise<Db> {
  const pool = new pg.Pool({ connectionString: databaseUrl });
  return drizzlePg(pool, { schema });
}

export async function createTestDb(): Promise<Db> {
  const db: Db = drizzlePglite(new PGlite(), { schema });
  await runMigrations(db);
  return db;
}
