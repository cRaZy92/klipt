import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import type { FriendList } from '@klipt/shared';
import { users } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
let alice: Record<string, string>;
let bob: Record<string, string>;
let bobId: string;

beforeEach(async () => {
  t = await makeTestApp();
  const a = await createUser(t.db, { username: 'alice' });
  const b = await createUser(t.db, { username: 'bob' });
  bobId = b.id;
  alice = (await sessionFor(t.db, a.id)).headers;
  bob = (await sessionFor(t.db, b.id, 'desktop')).headers;
});
afterEach(() => t.close());

const add = (name: string, h: Record<string, string>) => t.app.inject({ method: 'POST', url: `/api/friends/${name}`, headers: h });
const remove = (name: string, h: Record<string, string>) => t.app.inject({ method: 'DELETE', url: `/api/friends/${name}`, headers: h });
const list = async (h: Record<string, string>) => (await t.app.inject({ url: '/api/friends', headers: h })).json() as FriendList;
const names = (refs: { username: string }[]) => refs.map((r) => r.username);

describe('friends', () => {
  it('request then accept makes friends', async () => {
    const sent = await add('bob', alice);
    expect(sent.statusCode).toBe(200);
    expect(sent.json()).toEqual({ state: 'outgoing' });
    expect(names((await list(alice)).outgoing)).toEqual(['bob']);
    expect(names((await list(bob)).incoming)).toEqual(['alice']);
    expect((await list(bob)).friends).toEqual([]);

    // Asking again changes nothing.
    expect((await add('bob', alice)).json()).toEqual({ state: 'outgoing' });

    // Bob asking back is accepting, case-insensitively.
    expect((await add('ALICE', bob)).json()).toEqual({ state: 'friends' });
    const a = await list(alice);
    const b = await list(bob);
    expect(names(a.friends)).toEqual(['bob']);
    expect(names(b.friends)).toEqual(['alice']);
    expect([...a.incoming, ...a.outgoing, ...b.incoming, ...b.outgoing]).toEqual([]);
    expect((await add('bob', alice)).json()).toEqual({ state: 'friends' });
  });

  it('delete declines, cancels and unfriends', async () => {
    await add('bob', alice);
    expect((await remove('alice', bob)).statusCode).toBe(204); // decline
    expect((await list(alice)).outgoing).toEqual([]);

    await add('bob', alice);
    expect((await remove('bob', alice)).statusCode).toBe(204); // cancel
    expect((await list(bob)).incoming).toEqual([]);

    await add('bob', alice);
    await add('alice', bob);
    expect((await remove('bob', alice)).statusCode).toBe(204); // unfriend
    expect((await list(alice)).friends).toEqual([]);
    expect((await list(bob)).friends).toEqual([]);

    // Removing someone you have nothing with is fine too.
    expect((await remove('bob', alice)).statusCode).toBe(204);
  });

  it('cannot friend yourself or unknown users', async () => {
    expect((await add('alice', alice)).statusCode).toBe(400);
    expect((await add('alice', alice)).json()).toMatchObject({ error: 'self' });
    expect((await add('nobody', alice)).statusCode).toBe(404);
    expect((await add('nobody', alice)).json()).toMatchObject({ error: 'unknown_user' });
    await t.db.update(users).set({ disabledAt: new Date() }).where(eq(users.id, bobId));
    expect((await add('bob', alice)).statusCode).toBe(404);
  });

  it('friends of a disabled user are not listed', async () => {
    await add('bob', alice);
    await add('alice', bob);
    await t.db.update(users).set({ disabledAt: new Date() }).where(eq(users.id, bobId));
    expect((await list(alice)).friends).toEqual([]);
  });

  it('needs a login', async () => {
    expect((await t.app.inject({ url: '/api/friends' })).statusCode).toBe(401);
  });
});
