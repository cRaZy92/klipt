import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { clips } from '../src/db/schema.js';
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
