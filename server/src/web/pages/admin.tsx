import type { AdminStats, AdminUser, Invite } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import { formatGB, relativeDate } from '../components.js';
import { Layout } from '../layout.js';

import { esc as e } from '../escape.js';

export function AdminPage(props: { user: AuthUser; stats: AdminStats; invites: Invite[]; users: AdminUser[] }): JSX.Element {
  const { stats } = props;
  return (
    <Layout title="Admin" user={props.user}>
      <h1>Admin</h1>
      <div class="stats">
        <div class="stat"><b>{stats.users}</b><span>users</span></div>
        <div class="stat"><b>{stats.clips}</b><span>clips</span></div>
        <div class="stat"><b>{formatGB(stats.storageBytes)}</b><span>storage</span></div>
      </div>

      <h2>Invites</h2>
      <button type="button" class="btn primary" data-action="create-invite">Create invite</button>
      <ul class="rows plain" id="invites">
        {props.invites.map((i) => (
          <li class="row" data-code={e(i.code)}>
            <div class="row-main">
              <code class="url">{e(i.url)}</code>
              <div class="meta"><span>Expires {e(new Date(i.expiresAt).toISOString().slice(0, 10))}</span></div>
            </div>
            <button type="button" class="btn small" data-action="copy" data-url={e(i.url)}>Copy</button>
            <button type="button" class="btn small danger" data-action="revoke-invite">Revoke</button>
          </li>
        ))}
      </ul>

      <h2>Users</h2>
      <ul class="rows plain">
        {props.users.map((u) => (
          <li class="row" data-user-id={u.id}>
            <div class="row-main">
              <a href={`/u/${encodeURIComponent(u.username)}`}><strong>{e(u.username)}</strong></a>
              {u.isAdmin ? <span class="badge accent">admin</span> : ''}
              {u.disabled ? <span class="badge">disabled</span> : ''}
              <div class="meta"><span>{u.clipCount} clips</span><span>joined {relativeDate(new Date(u.createdAt))}</span></div>
            </div>
            {u.id === props.user.id ? (
              ''
            ) : (
              <button type="button" class="btn small" data-action="toggle-user" data-disabled={String(u.disabled)}>
                {u.disabled ? 'Enable' : 'Disable'}
              </button>
            )}
          </li>
        ))}
      </ul>
    </Layout>
  );
}
