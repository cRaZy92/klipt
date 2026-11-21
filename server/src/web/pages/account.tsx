import type { SessionInfo } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import { relativeDate } from '../components.js';
import { esc as e } from '../escape.js';
import { Layout } from '../layout.js';

/** The device a session reported (desktop app version and OS), if it did. */
export interface AccountSession extends SessionInfo {
  appVersion: string | null;
  os: string | null;
}

/** Your name, password, signed-in devices, and a way to send a problem or an idea to the admin. */
export function AccountPage(props: { user: AuthUser; sessions: AccountSession[] }): JSX.Element {
  const { user } = props;
  return (
    <Layout title="Account" user={user} nav="account">
      <div class="account-page">
        <h1>Account</h1>

        <section class="panel">
          <h2>Name</h2>
          <p class="muted">
            Friends see this name on your clips. You still log in as <strong>{e(user.username)}</strong>.
          </p>
          <form class="form" data-form="profile">
            <label>
              Display name
              <input name="displayName" value={e(user.customName ?? '')} placeholder={e(user.username)} maxlength="32" autocomplete="nickname" />
            </label>
            <p class="error" role="alert" hidden></p>
            <button class="btn primary" type="submit">Save name</button>
          </form>
        </section>

        <section class="panel">
          <h2>Password</h2>
          <p class="muted">Changing it signs you out everywhere else, including the app on your other PCs.</p>
          <form class="form" data-form="password">
            <input type="text" name="username" value={e(user.username)} autocomplete="username" hidden />
            <label>Current password<input name="currentPassword" type="password" autocomplete="current-password" required maxlength="1024" /></label>
            <label>New password<input name="newPassword" type="password" autocomplete="new-password" required minlength="8" maxlength="1024" /></label>
            <label>New password again<input name="confirmPassword" type="password" autocomplete="new-password" required minlength="8" maxlength="1024" /></label>
            <p class="error" role="alert" hidden></p>
            <button class="btn primary" type="submit">Change password</button>
          </form>
        </section>

        <section class="panel">
          <h2>Report a problem or suggest something</h2>
          <p class="muted">
            Goes to the admin. For a problem with the app, use <strong>Report a problem</strong> in the app's
            Settings instead: it sends the app's log too.
          </p>
          <form class="form" data-form="feedback">
            <div class="choice" role="radiogroup" aria-label="Kind">
              <label><input type="radio" name="kind" value="bug" checked /> Problem</label>
              <label><input type="radio" name="kind" value="idea" /> Idea</label>
            </div>
            <label>
              What happened, or what would you like?
              <textarea name="message" rows="5" required maxlength="10000"></textarea>
            </label>
            <p class="error" role="alert" hidden></p>
            <button class="btn primary" type="submit">Send</button>
          </form>
        </section>

        <section>
          <h2>Signed in on</h2>
          <ul class="rows plain">
            {props.sessions.map((s) => (
              <li class="row session" data-session-id={s.id}>
                <div class="row-main">
                  <strong>{e(s.label ?? (s.kind === 'web' ? 'Web' : 'Desktop'))}</strong>
                  <span class="badge">{s.kind === 'web' ? 'website' : 'app'}</span>
                  {s.current ? <span class="badge accent">this device</span> : ''}
                  <div class="meta">
                    <span>Last used {relativeDate(new Date(s.lastUsedAt))}</span>
                    {s.appVersion ? <span>Klipt {e(s.appVersion)}</span> : ''}
                    {s.os ? <span>{e(s.os)}</span> : ''}
                  </div>
                </div>
                {s.current ? '' : <button type="button" class="btn small danger" data-action="revoke-session">Sign out</button>}
              </li>
            ))}
          </ul>
        </section>
      </div>
    </Layout>
  );
}
