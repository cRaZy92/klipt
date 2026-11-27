import { describe, expect, it } from 'vitest';
import { createTestDb } from '../src/db/client.js';
import { sql } from 'drizzle-orm';
import { clips, friendships, users } from '../src/db/schema.js';

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

describe('migration 0017 (public Klipt)', () => {
  async function statement(marker: string): Promise<string> {
    const { readdir, readFile } = await import('node:fs/promises');
    const dir = new URL('../src/db/migrations/', import.meta.url);
    const file = (await readdir(dir)).find((f) => f.startsWith('0017_') && f.endsWith('.sql'));
    if (!file) throw new Error('no 0017 migration');
    const sqlText = await readFile(new URL(file, dir), 'utf8');
    const found = sqlText.split('--> statement-breakpoint').find((s) => s.includes(marker));
    if (!found) throw new Error(`no statement marked ${marker}`);
    return found;
  }

  it('backfill makes every active pair friends', async () => {
    const db = await createTestDb();
    const mk = (name: string, disabled = false) =>
      db.insert(users).values({ username: name, usernameKey: name, passwordHash: 'x', disabledAt: disabled ? new Date() : null }).returning();
    const [[a], [b], [c], [d]] = await Promise.all([mk('a'), mk('b'), mk('c'), mk('d', true)]);
    await db.execute(sql.raw(await statement('-- klipt:backfill-friends')));
    const rows = await db.select().from(friendships);
    expect(rows).toHaveLength(3);
    for (const r of rows) {
      expect(r.status).toBe('accepted');
      expect(r.userA < r.userB).toBe(true);
      expect([r.userA, r.userB]).not.toContain(d!.id);
    }
    expect(new Set(rows.flatMap((r) => [r.userA, r.userB]))).toEqual(new Set([a!.id, b!.id, c!.id]));
  });

  it('feed clips become friends clips', async () => {
    const db = await createTestDb();
    const [u] = await db.insert(users).values({ username: 'a', usernameKey: 'a', passwordHash: 'x' }).returning();
    await db.execute(sql`insert into clips (id, user_id, title, visibility, duration_ms, width, height, size_bytes) values ('AAAAAAAAAAAA', ${u!.id}, 't', 'feed', 1, 1, 1, 1)`);
    await db.execute(sql.raw(await statement('-- klipt:feed-to-friends')));
    const [clip] = await db.select().from(clips);
    expect(clip!.visibility).toBe('friends');
  });

  it('new clips default to friends', async () => {
    const db = await createTestDb();
    const [u] = await db.insert(users).values({ username: 'a', usernameKey: 'a', passwordHash: 'x' }).returning();
    await db.execute(sql`insert into clips (id, user_id, title, duration_ms, width, height, size_bytes) values ('AAAAAAAAAAAB', ${u!.id}, 't', 1, 1, 1, 1)`);
    const [clip] = await db.select().from(clips);
    expect(clip!.visibility).toBe('friends');
    const [me] = await db.select().from(users);
    expect(me!.profileVisibility).toBe('friends');
  });
});
