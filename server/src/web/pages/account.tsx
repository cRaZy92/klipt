import type { SessionInfo } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import { relativeDate } from '../components.js';
import { esc as e } from '../escape.js';
import { ACCENTS, ACHIEVEMENTS, BANNERS, FRAMES, THEMES, type CosmeticId } from '@klipt/shared';
import { avatarUrl } from '../../users/ref.js';
import { Avatar, Layout } from '../layout.js';

/** Why a cosmetic is locked: the achievement that unlocks it (a hidden one stays a secret). */
function unlocksWith(id: CosmeticId): string {
  const a = ACHIEVEMENTS.find((x) => x.reward === id);
  if (!a) return 'Locked';
  return a.hidden ? 'Unlocks with a hidden achievement' : `Unlocks with ${a.name}`;
}

/** One choice in the looks form: a radio, disabled (with what unlocks it) while locked. */
function Choice(props: { kind: 'accent' | 'theme' | 'frame' | 'banner'; id: string | null; name: string; checked: boolean; unlocked: Set<string>; children?: JSX.Element }): JSX.Element {
  const cid = `${props.kind}:${props.id}` as CosmeticId;
  const locked = props.id !== null && !props.unlocked.has(cid);
  return (
    <label class={`look-choice look-${props.kind}${locked ? ' locked' : ''}`} title={e(locked ? `${props.name}: ${unlocksWith(cid)}` : props.name)}>
      <input type="radio" name={props.kind} value={props.id ?? ''} checked={props.checked} disabled={locked} />
      {props.children ?? ''}
      <span class="look-name">{e(props.name)}</span>
      {locked ? <span class="look-lock" aria-label={e(unlocksWith(cid))}>🔒</span> : ''}
    </label>
  );
}

/** The device a session reported (desktop app version and OS), if it did. */
export interface AccountSession extends SessionInfo {
  appVersion: string | null;
  os: string | null;
}

/** Your name, password, signed-in devices, and a way to send a problem or an idea to the admin. */
export function AccountPage(props: { user: AuthUser; sessions: AccountSession[]; unlocked: CosmeticId[] }): JSX.Element {
  const { user } = props;
  const unlocked = new Set<string>(props.unlocked);
  const picture = avatarUrl({ id: user.id, avatarVersion: user.avatarVersion }, '');
  const looks = user.looks;
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
          <h2>Who sees your profile</h2>
          <p class="muted">Also the default for new uploads. You can still pick per clip, and older clips stay as they are.</p>
          <form class="form choice-list" data-form="profile-visibility">
            <label>
              <input type="radio" name="profileVisibility" value="friends" checked={user.profileVisibility === 'friends'} />
              <span><strong>Friends</strong> — others see only your name, picture and public clips</span>
            </label>
            <label>
              <input type="radio" name="profileVisibility" value="public" checked={user.profileVisibility === 'public'} />
              <span><strong>Public</strong> — anyone with the link sees your profile and public clips</span>
            </label>
            <p class="error" role="alert" hidden></p>
          </form>
        </section>

        <section class="panel">
          <h2>Picture</h2>
          <p class="muted">Friends see it on your clips, albums and profile. Any JPEG, PNG or WebP picture.</p>
          <form class="form avatar-form" data-form="avatar">
            <Avatar name={user.displayName} url={picture} frame={looks.frame} size="lg" />
            <input type="file" name="picture" accept="image/jpeg,image/png,image/webp" hidden />
            <div class="avatar-actions">
              <button class="btn primary" type="button" data-action="choose-avatar">Choose picture</button>
              {picture ? <button class="btn ghost" type="button" data-action="remove-avatar">Remove</button> : ''}
            </div>
            <p class="error" role="alert" hidden></p>
          </form>
        </section>

        <section class="panel">
          <h2>Looks</h2>
          <p class="muted">
            Colours and themes change what you see. Frames and banners are what friends see on you. Achievements unlock
            more of them.
          </p>
          <form class="form looks-form" data-form="looks">
            <fieldset>
              <legend>Accent colour</legend>
              <div class="look-row">
                {ACCENTS.map((a) => (
                  <Choice kind="accent" id={a.id} name={a.name} checked={looks.accent === a.id} unlocked={unlocked}>
                    <span class="swatch" style={`--c:${a.color}`}></span>
                  </Choice>
                ))}
              </div>
            </fieldset>
            <fieldset>
              <legend>Theme</legend>
              <div class="look-row">
                {THEMES.map((t) => (
                  <Choice kind="theme" id={t.id} name={t.name} checked={looks.theme === t.id} unlocked={unlocked}>
                    <span class="theme-chip" style={`--b:${t.tokens['--bg']};--s:${t.tokens['--surface-3']};--t:${t.tokens['--text']}`}></span>
                  </Choice>
                ))}
              </div>
            </fieldset>
            <fieldset>
              <legend>Avatar frame</legend>
              <div class="look-row">
                <Choice kind="frame" id={null} name="None" checked={looks.frame === null} unlocked={unlocked} />
                {FRAMES.map((f) => (
                  <Choice kind="frame" id={f.id} name={f.name} checked={looks.frame === f.id} unlocked={unlocked}>
                    <Avatar name={user.displayName} url={picture} frame={f.id} size="md" />
                  </Choice>
                ))}
              </div>
            </fieldset>
            <fieldset>
              <legend>Profile banner</legend>
              <div class="look-row">
                <Choice kind="banner" id={null} name="None" checked={looks.banner === null} unlocked={unlocked} />
                {BANNERS.map((b) => (
                  <Choice kind="banner" id={b.id} name={b.name} checked={looks.banner === b.id} unlocked={unlocked}>
                    <span class={`banner-chip banner-${b.id}`}></span>
                  </Choice>
                ))}
              </div>
            </fieldset>
            <p class="error" role="alert" hidden></p>
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
