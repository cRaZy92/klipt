import { sql } from 'drizzle-orm';
import type { Config } from '../config.js';
import type { Db } from '../db/client.js';
import { users } from '../db/schema.js';
import { hashPassword } from '../auth/passwords.js';

export async function ensureAdmin(
  db: Db,
  config: Pick<Config, 'adminUsername' | 'adminPassword'>,
): Promise<'created' | 'exists'> {
  const [{ n }] = await db.select({ n: sql<number>`count(*)::int` }).from(users);
  if (n! > 0) return 'exists';
  if (!config.adminUsername || !config.adminPassword) {
    throw new Error('ADMIN_USERNAME and ADMIN_PASSWORD must be set on first start');
  }
  await db.insert(users).values({
    username: config.adminUsername,
    usernameKey: config.adminUsername.toLowerCase(),
    passwordHash: await hashPassword(config.adminPassword),
    isAdmin: true,
  });
  return 'created';
}
