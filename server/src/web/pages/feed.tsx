import type { GameCount } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner } from '../../clips/repo.js';
import { ClipGrid, emptyText, FeedFilters, feedBase, GameFilter, Pager, SearchBox, type KindFilter } from '../components.js';
import { Layout } from '../layout.js';

export function FeedPage(props: {
  user: AuthUser;
  items: ClipWithOwner[];
  nextCursor: string | null;
  games: GameCount[];
  game?: string;
  search?: string;
  withMe?: boolean;
  kind?: KindFilter;
  favorites?: boolean;
}): JSX.Element {
  const base = feedBase(!!props.withMe, props.kind, !!props.favorites);
  return (
    <Layout title="Feed" user={props.user} nav="feed">
      <div class="page-head">
        <h1>Feed</h1>
        <SearchBox basePath="/" search={props.search} game={props.game} withMe={props.withMe} kind={props.kind} favorites={props.favorites} />
      </div>
      <FeedFilters withMe={!!props.withMe} kind={props.kind} favorites={props.favorites} />
      <GameFilter games={props.games} active={props.game} basePath={base} search={props.search} />
      <ClipGrid
        items={props.items}
        empty={emptyText(
          props.search,
          props.favorites
            ? 'No favorites yet. Open a clip and press Favorite to keep it here.'
            : props.withMe
              ? 'Nobody has tagged you in a clip yet.'
              : 'No clips yet. Save one with the Klipt desktop app (Get the app, at the top).',
        )}
      />
      <Pager nextCursor={props.nextCursor} basePath={base} game={props.game} search={props.search} />
    </Layout>
  );
}
