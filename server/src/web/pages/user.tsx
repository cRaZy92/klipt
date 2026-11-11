import { esc as e } from '../escape.js';
import type { GameCount } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner, ProfileStats } from '../../clips/repo.js';
import { ClipGrid, emptyText, GameFilter, Pager, SearchBox, viewsLabel } from '../components.js';
import { Layout } from '../layout.js';

export function UserPage(props: {
  user: AuthUser;
  username: string;
  items: ClipWithOwner[];
  nextCursor: string | null;
  games: GameCount[];
  game?: string;
  search?: string;
  stats: ProfileStats;
}): JSX.Element {
  const { stats } = props;
  const base = `/u/${encodeURIComponent(props.username)}`;
  return (
    <Layout title={props.username} user={props.user}>
      <h1>{e(props.username)}</h1>
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
