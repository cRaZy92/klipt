import { esc as e } from '../escape.js';
import { Layout, LogoMark } from '../layout.js';

/** A password reset link from an admin. */
export function ResetPage(props: { code: string; username: string | null }): JSX.Element {
  return (
    <Layout title="New password" user={null}>
      <div class="center-card">
        {props.username ? (
          <>
            <LogoMark size={44} />
            <h1>New password</h1>
            <p class="muted">
              For <strong>{e(props.username)}</strong>. You'll be signed out everywhere, then logged in here.
            </p>
            <form data-form="reset" data-next="/" class="form">
              <input type="hidden" name="code" value={e(props.code)} />
              <input type="text" name="username" value={e(props.username)} autocomplete="username" hidden />
              <label>New password<input name="newPassword" type="password" autocomplete="new-password" required minlength="8" maxlength="1024" autofocus /></label>
              <label>New password again<input name="confirmPassword" type="password" autocomplete="new-password" required minlength="8" maxlength="1024" /></label>
              <p class="error" role="alert" hidden></p>
              <button class="btn primary" type="submit">Set password</button>
            </form>
          </>
        ) : (
          <>
            <h1>Link not valid</h1>
            <p class="muted">This password link was used already or has expired. Ask your admin for a new one.</p>
            <a class="btn" href="/login">Log in</a>
          </>
        )}
      </div>
    </Layout>
  );
}
