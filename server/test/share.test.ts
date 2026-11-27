import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { eq } from 'drizzle-orm';
import { clips, friendships } from '../src/db/schema.js';
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
  });

  it('a video embed says who clipped it through oEmbed, since Discord hides og:description there', async () => {
    const u = await createUser(t.db, { username: 'alice' });
    await addClip(u.id, 'shareclip020', { gameName: 'CS2' });
    const page = await get('/c/shareclip020');
    expect(page.body).toContain('<link rel="alternate" type="application/json+oembed" href="http://localhost/c/shareclip020/oembed"');
    const res = await get('/c/shareclip020/oembed');
    expect(res.statusCode).toBe(200);
    expect(res.json()).toMatchObject({
      type: 'link',
      title: 'Ace',
      author_name: 'CS2 · clipped by alice',
      author_url: 'http://localhost/u/alice',
      provider_name: 'Klipt',
    });
    expect((await get('/c/nosuchclip0/oembed')).statusCode).toBe(404);
  });

  it('a long clip without an embed copy still embeds its full video when Discord can fetch it in time', async () => {
    const u = await createUser(t.db, { username: 'frank' });
    await addClip(u.id, 'shareclip011', { durationMs: 20 * 60_000, width: 1920, height: 1080, sizeBytes: 95 * 1024 * 1024 });
    const res = await get('/c/shareclip011');
    expect(res.body).toContain('<meta property="og:video" content="http://localhost/media/shareclip011.mp4"');
    expect(res.body).toContain('content="player"');
  });

  it('a huge clip without an embed copy embeds its thumbnail only, as a large image', async () => {
    const u = await createUser(t.db, { username: 'erin' });
    // A long upload before compression: far too big for Discord to fetch in time.
    await addClip(u.id, 'shareclip010', { durationMs: 20 * 60_000, width: 1920, height: 1080, sizeBytes: 680 * 1024 * 1024 });
    const res = await get('/c/shareclip010');
    expect(res.statusCode).toBe(200);
    expect(res.body).not.toContain('og:video');
    expect(res.body).toContain('<meta property="og:image" content="http://localhost/media/shareclip010.jpg"');
    expect(res.body).toContain('<meta property="og:image:width" content="1920" />');
    expect(res.body).toContain('<meta name="twitter:card" content="summary_large_image" />');
    expect(res.body).toContain('<meta property="og:title" content="Ace" />');
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
    expect(res.body).toMatch(/<a[^>]*class="btn[^"]*"[^>]*href="http:\/\/localhost\/media\/shareclip006\.mp4\?download=1"[^>]*>[\s\S]*?Download<\/a>/);
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
    expect(mine).toMatch(/data-role="views">2 views</);
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
    const deleteButton = /<aside[^>]*data-clip-id="shareclip005"[^>]*>[\s\S]*data-action="delete"[^>]*data-redirect="\/"/;
    expect(await view(owner.id)).toMatch(deleteButton);
    expect(await view(admin.id)).toMatch(deleteButton);
    expect(await view(other.id)).not.toContain('data-action="delete"');
    expect(await view()).not.toContain('data-action="delete"');
  });
});

describe('signing up from a share link', () => {
  it('offers logged-out visitors an account beside the clip, not logged-in ones', async () => {
    const owner = await createUser(t.db, { username: 'kim' });
    await addClip(owner.id, 'joinclip0001', { visibility: 'link' });
    const page = await get('/c/joinclip0001');
    expect(page.body).toContain('Create an account');
    expect(page.body).toMatch(/<form data-form="register" data-next="\/get-app\?welcome=1&amp;next=%2Fc%2Fjoinclip0001"/);
    expect(page.body).not.toContain('name="clipId"');
    expect(page.body).toContain('href="/login?next=%2Fc%2Fjoinclip0001"');
    const { cookie } = await sessionFor(t.db, owner.id);
    const mine = await t.app.inject({ url: '/c/joinclip0001', headers: { cookie } });
    expect(mine.body).not.toContain('data-form="register"');
  });

  it("signing up there doesn't make you the clip owner's friend", async () => {
    const owner = await createUser(t.db, { username: 'lena' });
    await addClip(owner.id, 'joinclip0001');
    const res = await t.app.inject({ method: 'POST', url: '/api/register', payload: { clipId: 'joinclip0001', username: 'Newbie', password: 'password123' } });
    expect(res.statusCode).toBe(200);
    expect(await t.db.select().from(friendships)).toEqual([]);
  });
});
