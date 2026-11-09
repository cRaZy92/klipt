import { describe, expect, it } from 'vitest';
import { createTestDb } from '../src/db/client.js';
import { users } from '../src/db/schema.js';

describe('schema', () => {
  it('rejects duplicate usernameKey', async () => {
    const db = await createTestDb();
    const row = { username: 'Adam', usernameKey: 'adam', passwordHash: 'x' };
    await db.insert(users).values(row);
    await expect(db.insert(users).values({ ...row, username: 'ADAM' })).rejects.toThrow();
  });
});
