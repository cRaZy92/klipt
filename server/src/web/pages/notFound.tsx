import type { AuthUser } from '../../auth/sessions.js';
import { Layout } from '../layout.js';

export function NotFoundPage(props: { user: AuthUser | null; message?: string }): JSX.Element {
  return (
    <Layout title="Not found" user={props.user}>
      <div class="center-card">
        <h1>Not found</h1>
        <p class="muted">{props.message ?? "That page doesn't exist, or you don't have access to it."}</p>
        <a class="btn" href="/">Back to the feed</a>
      </div>
    </Layout>
  );
}
