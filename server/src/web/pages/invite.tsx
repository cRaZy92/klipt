import { esc as e } from '../escape.js';
import { Layout } from '../layout.js';

export function InvitePage(props: { code: string; valid: boolean }): JSX.Element {
  return (
    <Layout title="Join Klipt" user={null}>
      <div class="center-card">
        {props.valid ? (
          <>
            <h1>Join Klipt</h1>
            <form data-form="register" data-next="/" class="form">
              <input type="hidden" name="inviteCode" value={props.code} />
              <label>Username<input name="username" autocomplete="username" required minlength="3" maxlength="24" pattern="[A-Za-z0-9_]{3,24}" title="3-24 letters, numbers or underscores" autofocus /></label>
              <label>Password<input name="password" type="password" autocomplete="new-password" required minlength="8" maxlength="1024" /></label>
              <p class="error" role="alert" hidden></p>
              <button class="btn primary" type="submit">Create account</button>
            </form>
            <p class="muted">Invite {e(props.code)}</p>
          </>
        ) : (
          <>
            <h1>Invite not valid</h1>
            <p class="muted">This invite link is no longer valid.</p>
            <a class="btn" href="/login">Log in</a>
          </>
        )}
      </div>
    </Layout>
  );
}
