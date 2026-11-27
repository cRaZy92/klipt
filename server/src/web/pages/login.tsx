import { esc as e } from '../escape.js';
import { Layout, LogoMark } from '../layout.js';

export function LoginPage(props: { next: string }): JSX.Element {
  return (
    <Layout title="Log in" user={null}>
      <div class="center-card">
        <LogoMark size={44} />
        <h1>Log in to Klipt</h1>
        <form data-form="login" data-next={e(props.next)} class="form">
          <label>Username<input name="username" autocomplete="username" required maxlength="100" autofocus /></label>
          <label>Password<input name="password" type="password" autocomplete="current-password" required maxlength="1024" /></label>
          <p class="error" role="alert" hidden></p>
          <button class="btn primary" type="submit">Log in</button>
        </form>
        <p class="muted">
          No account? <a href="/signup">Sign up</a>
        </p>
      </div>
    </Layout>
  );
}
