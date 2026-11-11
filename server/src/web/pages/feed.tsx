import type { GameCount } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner } from '../../clips/repo.js';
import { ClipGrid, emptyText, GameFilter, Pager, SearchBox, WithMeChip } from '../components.js';
import { Layout } from '../layout.js';

export function FeedPage(props: {
  user: AuthUser;
  items: ClipWithOwner[];
  nextCursor: string | null;
  games: GameCount[];
  game?: string;
  search?: string;
  withMe?: boolean;
}): JSX.Element {
  const base = props.withMe ? '/?with=me' : '/';
  return (
    <Layout title="Feed" user={props.user}>
      <h1>Feed</h1>
      <SearchBox basePath="/" search={props.search} game={props.game} withMe={props.withMe} />
      <WithMeChip active={!!props.withMe} />
      <GameFilter games={props.games} active={props.game} basePath={base} search={props.search} />
      <ClipGrid
        items={props.items}
        empty={emptyText(props.search, props.withMe ? "Nobody has tagged you in a clip yet." : 'No clips yet. Upload one from the Klipt desktop app.')}
      />
      <Pager nextCursor={props.nextCursor} basePath={base} game={props.game} search={props.search} />
    </Layout>
  );
}
