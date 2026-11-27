import type { Referral, UserRef } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import { esc as e } from '../escape.js';
import { Avatar, Layout } from '../layout.js';

function Person(props: { who: UserRef; children?: JSX.Element | JSX.Element[] | string }): JSX.Element {
  return (
    <li class="row person" data-username={e(props.who.username)}>
      <a class="row-main person-link" href={`/u/${encodeURIComponent(props.who.username)}`}>
        <Avatar name={props.who.displayName} url={props.who.avatarUrl} frame={props.who.frame} />
        <span>
          <strong>{e(props.who.displayName)}</strong>
          {props.who.displayName !== props.who.username ? <span class="muted"> @{e(props.who.username)}</span> : ''}
        </span>
      </a>
      {props.children ?? ''}
    </li>
  );
}

const act = (action: string, username: string, label: string, cls = 'btn small') => (
  <button type="button" class={cls} data-action={action} data-username={e(username)}>
    {label}
  </button>
);

const until = (iso: string) => new Date(iso).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', timeZone: 'Europe/Bratislava' });

/** Your friends, requests either way, adding by username, and referral links. */
export function FriendsPage(props: {
  user: AuthUser;
  friends: UserRef[];
  incoming: UserRef[];
  outgoing: UserRef[];
  referrals: Referral[];
  notice: 'own' | 'invalid' | null;
}): JSX.Element {
  return (
    <Layout title="Friends" user={props.user} nav="friends">
      <div class="friends-page">
        <h1>Friends</h1>
        {props.notice === 'own' ? <p class="notice">That's your own link. Send it to a friend instead.</p> : ''}
        {props.notice === 'invalid' ? <p class="notice">That link was already used or expired. Ask for a new one.</p> : ''}

        {props.incoming.length > 0 ? (
          <section class="panel" data-section="incoming">
            <h2>Requests</h2>
            <ul class="rows plain">
              {props.incoming.map((who) => (
                <Person who={who}>
                  {act('friend-add', who.username, 'Accept', 'btn small primary')}
                  {act('friend-remove', who.username, 'Decline')}
                </Person>
              ))}
            </ul>
          </section>
        ) : (
          ''
        )}

        <section class="panel">
          <h2>Bring a friend</h2>
          <p class="muted">
            Make a link and send it. Whoever signs up with it (or opens it while logged in) becomes your friend. Each link
            works once, for 14 days.
          </p>
          <button type="button" class="btn primary" data-action="make-referral">Make a link</button>
          <ul class="rows plain" id="referrals">
            {props.referrals.map((r) => (
              <li class="row" data-code={e(r.code)}>
                <div class="row-main">
                  <code class="url">{e(r.url)}</code>
                  <div class="meta"><span>Works until {until(r.expiresAt)}</span></div>
                </div>
                <button type="button" class="btn small" data-action="copy" data-url={e(r.url)}>Copy</button>
                <button type="button" class="btn small danger" data-action="revoke-referral">Revoke</button>
              </li>
            ))}
          </ul>
          <form class="form inline-form" data-form="add-friend">
            <label>
              Or add by username
              <input name="username" autocomplete="off" required maxlength="24" pattern="[A-Za-z0-9_]{3,24}" placeholder="username" />
            </label>
            <button class="btn" type="submit">Send request</button>
            <p class="error" role="alert" hidden></p>
          </form>
        </section>

        <section class="panel" data-section="friends">
          <h2>Your friends{props.friends.length > 0 ? <span class="muted"> · {props.friends.length}</span> : ''}</h2>
          {props.friends.length === 0 ? <p class="empty">No friends yet. Make a link above and send it to someone.</p> : ''}
          <ul class="rows plain">
            {props.friends.map((who) => (
              <Person who={who}>{act('friend-remove', who.username, 'Unfriend')}</Person>
            ))}
          </ul>
        </section>

        {props.outgoing.length > 0 ? (
          <section class="panel" data-section="outgoing">
            <h2>Waiting for an answer</h2>
            <ul class="rows plain">
              {props.outgoing.map((who) => (
                <Person who={who}>{act('friend-remove', who.username, 'Cancel')}</Person>
              ))}
            </ul>
          </section>
        ) : (
          ''
        )}
      </div>
    </Layout>
  );
}
