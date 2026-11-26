import { esc as e } from '../escape.js';
import type { GameCount } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner, ProfileStats } from '../../clips/repo.js';
import { ClipGrid, emptyText, GameFilter, Pager, SearchBox, viewsLabel } from '../components.js';
import { AvatarOf, Layout } from '../layout.js';
import { BadgeStrip } from '../badges.js';
import type { EarnedAchievement } from '@klipt/shared';
import type { UserRefRow } from '../../users/ref.js';

export function UserPage(props: {
  user: AuthUser;
  username: string;
  displayName: string;
  items: ClipWithOwner[];
  nextCursor: string | null;
  games: GameCount[];
  game?: string;
  search?: string;
  stats: ProfileStats;
  /** The profile's owner as others see them. */
  owner: UserRefRow;
  /** Their equipped banner, if any. */
  banner: string | null;
  badges: EarnedAchievement[];
}): JSX.Element {
  const { stats } = props;
  const base = `/u/${encodeURIComponent(props.username)}`;
  return (
    <Layout title={props.displayName} user={props.user}>
      <div class={`profile-head${props.banner ? ` has-banner banner-${props.banner}` : ''}`}>
        <AvatarOf who={props.owner} size="lg" />
        <div>
          <h1>{e(props.displayName)}</h1>
          {props.displayName !== props.username ? <p class="muted handle">@{e(props.username)}</p> : ''}
        </div>
      </div>
      <BadgeStrip earned={props.badges} />
      {stats.clips > 0 ? (
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
