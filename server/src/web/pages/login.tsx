import { esc as e } from '../escape.js';
import { Layout } from '../layout.js';

export function LoginPage(props: { next: string }): JSX.Element {
  return (
    <Layout title="Log in" user={null}>
      <div class="center-card">
        <h1>Log in to Klipt</h1>
        <form data-form="login" data-next={e(props.next)} class="form">
          <label>Username<input name="username" autocomplete="username" required maxlength="100" autofocus /></label>
          <label>Password<input name="password" type="password" autocomplete="current-password" required maxlength="1024" /></label>
          <p class="error" role="alert" hidden></p>
          <button class="btn primary" type="submit">Log in</button>
        </form>
        <p class="muted">Klipt is invite-only. Ask a friend for an invite link.</p>
      </div>
    </Layout>
  );
}
