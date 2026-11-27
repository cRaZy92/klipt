import type { AuthUser } from '../../auth/sessions.js';
import { esc as e } from '../escape.js';
import { Layout, LogoMark } from '../layout.js';

/** A referral link opened while logged in: becoming friends takes a click. */
export function ReferralPage(props: { user: AuthUser; code: string; sender: { username: string; displayName: string } }): JSX.Element {
  return (
    <Layout title="Become friends" user={props.user}>
      <div class="center-card">
        <LogoMark size={44} />
        <h1>
          Become friends with <strong>{e(props.sender.displayName)}</strong>?
        </h1>
        <p class="muted">You'll see each other's clips in the feed. You can unfriend any time.</p>
        <p class="error" role="alert" hidden></p>
        <div class="home-cta">
          <button type="button" class="btn primary" data-action="accept-referral" data-code={e(props.code)}>
            Become friends
          </button>
          <a class="btn" href="/">Not now</a>
        </div>
      </div>
    </Layout>
  );
}
