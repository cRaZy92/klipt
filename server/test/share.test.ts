import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { clips, invites, users } from '../src/db/schema.js';
import { createUser, makeTestApp, sessionFor, type TestApp } from './helpers.js';

let t: TestApp;
beforeEach(async () => {
  t = await makeTestApp();
});
afterEach(async () => {
  await t.close();
});

async function addClip(userId: string, id: string, extra: Partial<typeof clips.$inferInsert> = {}) {
  await t.db.insert(clips).values({ id, userId, title: 'Ace', durationMs: 1000, width: 1280, height: 720, sizeBytes: 10, ...extra });
}
const get = (url: string) => t.app.inject({ method: 'GET', url });

describe('share page', () => {
  it('serves a feed clip publicly with embed metadata', async () => {
    const u = await createUser(t.db, { username: 'alice' });
    await addClip(u.id, 'shareclip001', { gameName: 'CS2' });
    const res = await get('/c/shareclip001');
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('<meta property="og:video" content="http://localhost/media/shareclip001.mp4"');
    expect(res.body).toContain('<meta property="og:image" content="http://localhost/media/shareclip001.jpg"');
    expect(res.body).toContain('content="CS2 · clipped by alice"');
    expect(res.body).toContain('content="player"');
    expect(res.body).toContain('name="robots" content="noindex"');
    expect(res.body).toMatch(/<video controls[^>]*playsinline[^>]*preload="metadata"/);
    // It starts by itself (app.js); the sound button waits hidden for browsers that block sound.
    expect(res.body).toMatch(/<video [^>]*data-autoplay/);
    expect(res.body).toMatch(/<button type="button" class="unmute" hidden aria-label="Turn the sound on">/);
  });

  it('serves link-only clips, escapes titles, omits game when absent', async () => {
    const u = await createUser(t.db, { username: 'bob' });
    await addClip(u.id, 'shareclip002', { visibility: 'link', title: 'A&B "x" <i>' });
    const res = await get('/c/shareclip002');
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('content="A&amp;B &quot;x&quot; &lt;i&gt;"');
    expect(res.body).toContain('property="og:description" content="bob"');
    expect(res.body).not.toContain('<i>');
  });

  it('shows the viewer name when logged in', async () => {
    const u = await createUser(t.db, { username: 'carol' });
    await addClip(u.id, 'shareclip003');
    const { cookie } = await sessionFor(t.db, u.id);
    const res = await t.app.inject({ method: 'GET', url: '/c/shareclip003', headers: { cookie } });
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('class="who"');
  });

  it('404s for deleted, unknown and malformed ids', async () => {
    const u = await createUser(t.db, { username: 'dave' });
    await addClip(u.id, 'shareclip004');
    const { token } = await sessionFor(t.db, u.id);
    const del = await t.app.inject({ method: 'DELETE', url: '/api/clips/shareclip004', headers: { authorization: `Bearer ${token}` } });
    expect(del.statusCode).toBe(204);
    for (const id of ['shareclip004', 'doesnotexist', 'short', 'bad!id!!!!!!!']) {
      const res = await get(`/c/${id}`);
      expect(res.statusCode).toBe(404);
      expect(res.body).toContain("doesn't exist or was deleted");
    }
  });

  it('offers a download of the video', async () => {
    const u = await createUser(t.db, { username: 'gina' });
    await addClip(u.id, 'shareclip006');
    const res = await get('/c/shareclip006');
    expect(res.body).toMatch(/<a[^>]*class="btn"[^>]*href="http:\/\/localhost\/media\/shareclip006\.mp4\?download=1"[^>]*>Download<\/a>/);
  });

  it('shows the view count to everyone and who watched it to the owner only', async () => {
    const owner = await createUser(t.db, { username: 'hana' });
    const friend = await createUser(t.db, { username: 'ivan' });
    await addClip(owner.id, 'shareclip007');
    const page = async (userId?: string) =>
      (await t.app.inject({ url: '/c/shareclip007', headers: userId ? { cookie: (await sessionFor(t.db, userId)).cookie } : {} })).body;
    expect(await page(owner.id)).toContain('No friends have watched it yet.');
    await t.app.inject({ method: 'POST', url: '/api/clips/shareclip007/view', headers: { cookie: (await sessionFor(t.db, friend.id)).cookie } });
    await t.app.inject({ method: 'POST', url: '/api/clips/shareclip007/view' });
    const mine = await page(owner.id);
    expect(mine).toMatch(/data-role="views"> · 2 views</);
    expect(mine).toMatch(/data-role="seen-by">Seen by ivan</);
    const theirs = await page(friend.id);
    expect(theirs).toContain('2 views');
    expect(theirs).not.toContain('data-role="seen-by"');
    expect(await page()).not.toContain('data-role="seen-by"');
    expect(theirs).toContain('data-view-url="/api/clips/shareclip007/view"');
  });

  it('shows Delete to the owner and admins only', async () => {
    const owner = await createUser(t.db, { username: 'erin' });
    const admin = await createUser(t.db, { username: 'root', isAdmin: true });
    const other = await createUser(t.db, { username: 'frank' });
    await addClip(owner.id, 'shareclip005');
    const view = async (userId?: string) =>
      (await t.app.inject({ method: 'GET', url: '/c/shareclip005', headers: userId ? { cookie: (await sessionFor(t.db, userId)).cookie } : {} })).body;
    const deleteButton = /<div[^>]*data-clip-id="shareclip005"[^>]*>[\s\S]*data-action="delete"[^>]*data-redirect="\/"/;
    expect(await view(owner.id)).toMatch(deleteButton);
    expect(await view(admin.id)).toMatch(deleteButton);
    expect(await view(other.id)).not.toContain('data-action="delete"');
    expect(await view()).not.toContain('data-action="delete"');
  });
});

describe('signing up from a share link', () => {
  const register = (payload: Record<string, unknown>) => t.app.inject({ method: 'POST', url: '/api/register', payload });
  const joinForm = /<form data-form="register" data-next="\/c\/joinclip0001"[\s\S]*name="clipId" value="joinclip0001"/;

  it('offers logged-out visitors an account beside the clip, not logged-in ones', async () => {
    const owner = await createUser(t.db, { username: 'kim' });
    await addClip(owner.id, 'joinclip0001', { visibility: 'link' });
    const page = await get('/c/joinclip0001');
    expect(page.body).toContain('Create an account');
    expect(page.body).toMatch(joinForm);
    expect(page.body).toContain('href="/login?next=%2Fc%2Fjoinclip0001"');
    const { cookie } = await sessionFor(t.db, owner.id);
    const mine = await t.app.inject({ url: '/c/joinclip0001', headers: { cookie } });
    expect(mine.body).not.toContain('data-form="register"');
  });

  it('creates the account, logs it in and records the clip owner as the inviter', async () => {
    const owner = await createUser(t.db, { username: 'lena' });
    await addClip(owner.id, 'joinclip0001');
    const res = await register({ clipId: 'joinclip0001', username: 'Newbie', password: 'password123' });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toMatchObject({ username: 'Newbie', isAdmin: false });
    const cookie = res.cookies.find((c) => c.name === 'klipt_session');
    const me = await t.app.inject({ url: '/api/me', headers: { cookie: `klipt_session=${cookie!.value}` } });
    expect(me.json()).toMatchObject({ username: 'Newbie' });
    const [inv] = await t.db.select().from(invites);
    expect(inv).toMatchObject({ createdBy: owner.id, usedBy: (me.json() as { id: string }).id });
    // The same link keeps working for the next friend.
    expect((await register({ clipId: 'joinclip0001', username: 'Second', password: 'password123' })).statusCode).toBe(200);
    expect(await t.db.select().from(invites)).toHaveLength(2);
  });

  it('keeps the usual username and password rules', async () => {
    const owner = await createUser(t.db, { username: 'mila' });
    await addClip(owner.id, 'joinclip0001');
    expect((await register({ clipId: 'joinclip0001', username: 'MILA', password: 'password123' })).json()).toEqual({ error: 'username_taken' });
    expect((await register({ clipId: 'joinclip0001', username: 'a b', password: 'password123' })).json()).toEqual({ error: 'invalid_username' });
    expect((await register({ clipId: 'joinclip0001', username: 'newone', password: 'short' })).json()).toEqual({ error: 'weak_password' });
    expect(await t.db.select().from(invites)).toEqual([]);
  });

  it('refuses unknown or deleted clips, both or neither code, and disabled owners', async () => {
    const owner = await createUser(t.db, { username: 'nora' });
    await addClip(owner.id, 'joinclip0001');
    await addClip(owner.id, 'joinclip0002');
    const ok = { username: 'joiner', password: 'password123' };
    expect((await register({ ...ok, clipId: 'nosuchclip01' })).json()).toEqual({ error: 'invalid_share' });
    expect((await register({ ...ok, clipId: 'bad' })).json()).toEqual({ error: 'invalid_request' });
    expect((await register(ok)).json()).toEqual({ error: 'invalid_request' });
    expect((await register({ ...ok, clipId: 'joinclip0001', inviteCode: 'x' })).json()).toEqual({ error: 'invalid_request' });

    const { headers } = await sessionFor(t.db, owner.id);
    await t.app.inject({ method: 'DELETE', url: '/api/clips/joinclip0002', headers });
    expect((await register({ ...ok, clipId: 'joinclip0002' })).json()).toEqual({ error: 'invalid_share' });

    await t.db.update(users).set({ disabledAt: new Date() }).where(eq(users.id, owner.id));
    expect((await get('/c/joinclip0001')).body).not.toContain('data-form="register"');
    expect((await register({ ...ok, clipId: 'joinclip0001' })).json()).toEqual({ error: 'invalid_share' });
    expect(await t.db.select().from(users).where(eq(users.usernameKey, 'joiner'))).toEqual([]);
  });

  it('is off with SHARE_SIGNUP=off', async () => {
    await t.close();
    t = await makeTestApp({ shareSignup: false });
    const owner = await createUser(t.db, { username: 'olga' });
    await addClip(owner.id, 'joinclip0001');
    expect((await get('/c/joinclip0001')).body).not.toContain('data-form="register"');
    expect((await register({ clipId: 'joinclip0001', username: 'joiner', password: 'password123' })).json()).toEqual({ error: 'invalid_share' });
  });
});
