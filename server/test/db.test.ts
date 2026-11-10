import { describe, expect, it } from 'vitest';
import { createTestDb } from '../src/db/client.js';
import { users } from '../src/db/schema.js';

describe('schema', () => {
  it('rejects duplicate usernameKey', async () => {
    const db = await createTestDb();
    const row = { username: 'Adam', usernameKey: 'adam', passwordHash: 'x' };
    await db.insert(users).values(row);
    const err = await db.insert(users).values({ ...row, username: 'ADAM' }).then(
      () => null,
      (e: unknown) => e as { code?: string; constraint?: string; cause?: { code?: string; constraint?: string } },
    );
    expect(err).not.toBeNull();
    const pgErr = err?.cause ?? err;
    expect(pgErr?.code).toBe('23505');
    expect(pgErr?.constraint).toBe('users_username_key_unique');
  });
});
