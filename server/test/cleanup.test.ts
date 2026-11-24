import { eq } from 'drizzle-orm';
import { beforeEach, describe, expect, it } from 'vitest';
import { cleanupExpired } from '../src/cleanup.js';
import { createTestDb, type Db } from '../src/db/client.js';
import { invites, passwordResets, sessions } from '../src/db/schema.js';
import { createUser } from './helpers.js';

const DAY = 24 * 60 * 60 * 1000;
const now = new Date('2026-11-24T12:00:00Z');
const ago = (days: number) => new Date(now.getTime() - days * DAY);

let db: Db;
beforeEach(async () => {
  db = await createTestDb();
});

const session = (userId: string, tokenHash: string, lastUsedDaysAgo: number, expiresAt: Date | null) =>
  db.insert(sessions).values({ userId, tokenHash, kind: 'web', lastUsedAt: ago(lastUsedDaysAgo), expiresAt });

describe('cleanupExpired', () => {
  it("removes expired logins but keeps each user's latest one, and never live ones", async () => {
    const alice = await createUser(db, { username: 'alice' });
    const bob = await createUser(db, { username: 'bob' });
    await session(alice.id, 'a-old', 90, ago(60));
    await session(alice.id, 'a-older', 120, ago(90));
    await session(alice.id, 'a-live', 1, new Date(now.getTime() + DAY));
    await session(alice.id, 'a-desktop', 200, null);
    await session(bob.id, 'b-old', 100, ago(70));
    await session(bob.id, 'b-last', 50, ago(20));

    const n = await cleanupExpired(db, now);
    expect(n.sessions).toBe(3);
    const left = (await db.select({ h: sessions.tokenHash }).from(sessions)).map((r) => r.h).sort();
    // bob's latest stays (his "last seen"), even though it expired.
    expect(left).toEqual(['a-desktop', 'a-live', 'b-last']);
    expect((await cleanupExpired(db, now)).sessions).toBe(0);
  });

  it('removes expired password links and unused expired or revoked invites; used invites stay', async () => {
    const admin = await createUser(db, { username: 'boss', isAdmin: true });
    const friend = await createUser(db, { username: 'friend' });
    await db.insert(passwordResets).values([
      { code: 'r-expired', userId: friend.id, createdBy: admin.id, expiresAt: ago(1) },
      { code: 'r-live', userId: friend.id, createdBy: admin.id, expiresAt: new Date(now.getTime() + DAY) },
    ]);
    await db.insert(invites).values([
      { code: 'i-expired', createdBy: admin.id, expiresAt: ago(1) },
      { code: 'i-revoked', createdBy: admin.id, expiresAt: new Date(now.getTime() + DAY), revokedAt: ago(1) },
      { code: 'i-live', createdBy: admin.id, expiresAt: new Date(now.getTime() + DAY) },
      { code: 'i-used', createdBy: admin.id, expiresAt: ago(30), usedBy: friend.id, usedAt: ago(31) },
    ]);

    expect(await cleanupExpired(db, now)).toEqual({ sessions: 0, passwordResets: 1, invites: 2 });
    expect((await db.select({ c: passwordResets.code }).from(passwordResets)).map((r) => r.c)).toEqual(['r-live']);
    const left = (await db.select({ c: invites.code }).from(invites)).map((r) => r.c).sort();
    expect(left).toEqual(['i-live', 'i-used']);
    expect(await db.select().from(invites).where(eq(invites.code, 'i-used'))).toHaveLength(1);
  });
});
