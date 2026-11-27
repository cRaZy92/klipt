import { esc as e } from '../escape.js';
import { Layout, LogoMark } from '../layout.js';

/** Make an account. `referrer`: who sent the referral link the visitor came from. */
export function SignupPage(props: { next: string; referrer: string | null }): JSX.Element {
  const after = props.next === '/' ? '/get-app?welcome=1' : `/get-app?welcome=1&next=${encodeURIComponent(props.next)}`;
  return (
    <Layout title="Sign up" user={null}>
      <div class="center-card">
        <LogoMark size={44} />
        <h1>Join Klipt</h1>
        {props.referrer ? (
          <p class="muted referral-note">
            <strong>{e(props.referrer)}</strong> invited you. You'll be friends as soon as your account is made.
          </p>
        ) : (
          ''
        )}
        <form data-form="signup" data-next={e(after)} class="form">
          <label>Username<input name="username" autocomplete="username" required minlength="3" maxlength="24" pattern="[A-Za-z0-9_]{3,24}" title="3-24 letters, numbers or underscores" autofocus /></label>
          <label>Password<input name="password" type="password" autocomplete="new-password" required minlength="8" maxlength="1024" /></label>
          <label>Password again<input name="confirmPassword" type="password" autocomplete="new-password" required minlength="8" maxlength="1024" /></label>
          <p class="error" role="alert" hidden></p>
          <button class="btn primary" type="submit">Create account</button>
        </form>
        <p class="muted">
          Already have an account? <a href={e(`/login?next=${encodeURIComponent(props.next)}`)}>Log in</a>
        </p>
      </div>
    </Layout>
  );
}
