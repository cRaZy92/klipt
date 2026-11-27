import { esc as e } from '../escape.js';
import type { FriendState, GameCount } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner, ProfileStats } from '../../clips/repo.js';
import { ClipGrid, emptyText, GameFilter, Pager, SearchBox, viewsLabel } from '../components.js';
import { AvatarOf, Layout } from '../layout.js';
import { BadgeStrip } from '../badges.js';
import { joinedLabel, type EarnedAchievement } from '@klipt/shared';
import type { UserRefRow } from '../../users/ref.js';

/** Add friend / Requested / Accept / Friends, for someone else's profile. */
function FriendButton(props: { state: FriendState | null; username: string; loginNext: string }): JSX.Element {
  const name = e(props.username);
  if (props.state === null) {
    return <a class="btn primary" href={`/login?next=${encodeURIComponent(props.loginNext)}`}>Log in to add</a>;
  }
  if (props.state === 'self') return <></>;
  const button = (action: string, label: string, cls = 'btn') => (
    <button type="button" class={cls} data-action={action} data-username={name}>
      {label}
    </button>
  );
  return (
    <div class="friend-button" data-friend-state={props.state}>
      {props.state === 'none' ? button('friend-add', 'Add friend', 'btn primary') : ''}
      {props.state === 'outgoing' ? <>{button('friend-remove', 'Requested · Cancel')}</> : ''}
      {props.state === 'incoming' ? <>{button('friend-add', 'Accept', 'btn primary')}{button('friend-remove', 'Decline')}</> : ''}
      {props.state === 'friends' ? <>
        <span class="badge">Friends</span>
        {button('friend-remove', 'Unfriend', 'btn small')}
      </> : ''}
    </div>
  );
}

export function UserPage(props: {
  user: AuthUser | null;
  username: string;
  displayName: string;
  items: ClipWithOwner[];
  nextCursor: string | null;
  games: GameCount[];
  game?: string;
  search?: string;
  /** Null: the full profile is hidden from this viewer (a Friends profile, not a friend). */
  stats: ProfileStats | null;
  /** Where the viewer stands with the owner; null when logged out. */
  friendState: FriendState | null;
  /** Where "Log in to add" comes back to. */
  loginNext: string;
  /** The profile's owner as others see them. */
  owner: UserRefRow;
  /** Their equipped banner, if any. */
  banner: string | null;
  badges: EarnedAchievement[];
  joinedAt: Date;
}): JSX.Element {
  const { stats } = props;
  const base = `/u/${encodeURIComponent(props.username)}`;
  return (
    <Layout title={props.displayName} user={props.user}>
      <div class={`profile-head${props.banner ? ` has-banner banner-${props.banner}` : ''}`}>
        <AvatarOf who={props.owner} size="lg" />
        <div>
          <h1>{e(props.displayName)}</h1>
          <p class="muted handle">
            {props.displayName !== props.username ? <>@{e(props.username)} · </> : ''}
            <span class="joined">{joinedLabel(props.joinedAt)}</span>
          </p>
        </div>
        <FriendButton state={props.friendState} username={props.username} loginNext={props.loginNext} />
      </div>
      {stats === null ? <p class="muted profile-private">Only friends see everything {e(props.displayName)} posts. These are their public clips.</p> : ''}
      <BadgeStrip earned={props.badges} />
      {stats && stats.clips > 0 ? (
        <div class="stats profile-stats">
          <div class="stat" data-stat="clips"><b>{stats.clips}</b><span>{stats.clips === 1 ? 'clip' : 'clips'}</span></div>
          <div class="stat" data-stat="views"><b>{stats.views}</b><span>{stats.views === 1 ? 'view' : 'views'}</span></div>
          {stats.top ? (
            <div class="stat top" data-stat="top"><a href={`/c/${e(stats.top.id)}`}><b>{e(stats.top.title)}</b></a><span>most watched · {viewsLabel(stats.top.views)}</span></div>
          ) : (
            ''
          )}
        </div>
      ) : (
        ''
      )}
      <SearchBox basePath={base} search={props.search} game={props.game} />
      <GameFilter games={props.games} active={props.game} basePath={base} search={props.search} />
      <ClipGrid items={props.items} empty={emptyText(props.search, 'No clips here yet.')} />
      <Pager nextCursor={props.nextCursor} basePath={base} game={props.game} search={props.search} />
    </Layout>
  );
}
