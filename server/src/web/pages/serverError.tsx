import type { AuthUser } from '../../auth/sessions.js';
import { Layout } from '../layout.js';

export function ServerErrorPage(props: { user: AuthUser | null }): JSX.Element {
  return (
    <Layout title="Error" user={props.user}>
      <div class="center-card">
        <h1>Something went wrong</h1>
        <p class="muted">Klipt hit an unexpected error. Please try again in a moment.</p>
        <a class="btn" href="/">Back to the feed</a>
      </div>
    </Layout>
  );
}
