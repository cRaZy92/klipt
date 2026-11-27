import type { AuthUser } from '../../auth/sessions.js';
import { esc as e } from '../escape.js';
import { Layout } from '../layout.js';

/** The Linux package's install command (the same script installs and updates). */
export const LINUX_INSTALL = 'sh -c "$(curl -fsSL https://raw.githubusercontent.com/cRaZy92/klipt/main/packaging/aur/update.sh)"';

/**
 * How to install the desktop app. New accounts land here; it's also
 * in the top bar. `next` is where "Continue" goes (the feed, or the clip someone signed up from).
 */
export function GetAppPage(props: { user: AuthUser | null; next: string; welcome: boolean }): JSX.Element {
  const { user, next, welcome } = props;
  const continueLabel = next.startsWith('/c/') ? 'Back to the clip' : 'Go to the feed';
  return (
    <Layout title="Get the app" user={user} nav="get-app">
      <div class="guide">
        <h1>{welcome && user ? `Welcome, ${e(user.displayName)}!` : 'Get the Klipt app'}</h1>
        <p class="lead muted">
          The Klipt app records your game in the background. Press a shortcut after a great moment and it saves the
          last 30 seconds, ready to upload and share here.
        </p>

        <ol class="steps">
          <li class="step">
            <h2>Download the installer</h2>
            <p>For Windows 10 and 11.</p>
            <p>
              <a class="btn primary" href="/get-app/windows">Download for Windows</a>
            </p>
          </li>

          <li class="step">
            <h2>Run it</h2>
            <p>
              Windows may show a blue <strong>"Windows protected your PC"</strong> window, because the installer isn't
              signed with a paid certificate yet. To continue:
            </p>
            <ol class="substeps">
              <li>
                Click <strong>More info</strong>.
              </li>
              <li>
                Click <strong>Run anyway</strong>.
              </li>
            </ol>
            <p class="muted">
              Klipt installs just for your Windows account, so it doesn't ask for admin rights. It updates itself
              from then on.
            </p>
          </li>

          <li class="step">
            <h2>Log in</h2>
            <p>
              Open Klipt and log in with {user ? <>your username <strong>{e(user.username)}</strong></> : 'your username'} and
              password.{user ? '' : <> No account yet? <a href="/signup">Sign up here</a>.</>}
            </p>
          </li>

          <li class="step">
            <h2>Save a clip</h2>
            <p>
              In a game, press <kbd>Alt</kbd> + <kbd>`</kbd> (the key under <kbd>Esc</kbd>) to save the last 30
              seconds, or <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>`</kbd> for a screenshot. You can change the
              shortcuts and the clip length in the app's Settings.
            </p>
          </li>
        </ol>

        <details class="linux">
          <summary>On Arch Linux?</summary>
          <p>Run this in a terminal. It builds and installs the Klipt package, and asks for your password once:</p>
          <p class="copy-line">
            <code>{e(LINUX_INSTALL)}</code>
            <button type="button" class="btn small" data-action="copy" data-url={e(LINUX_INSTALL)}>
              Copy
            </button>
          </p>
          <p class="muted">The app tells you when there's a newer version, with the command to update.</p>
        </details>

        {user ? (
          <p class="guide-next">
            <a class="btn" href={e(next)}>
              {continueLabel}
            </a>
          </p>
        ) : (
          ''
        )}
      </div>
    </Layout>
  );
}
