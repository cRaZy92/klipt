import type { GameCount } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner } from '../../clips/repo.js';
import { ClipGrid, emptyText, GameFilter, Pager, SearchBox } from '../components.js';
import { Layout } from '../layout.js';

export function FeedPage(props: {
  user: AuthUser;
  items: ClipWithOwner[];
  nextCursor: string | null;
  games: GameCount[];
  game?: string;
  search?: string;
}): JSX.Element {
  return (
    <Layout title="Feed" user={props.user}>
      <h1>Feed</h1>
      <SearchBox basePath="/" search={props.search} game={props.game} />
      <GameFilter games={props.games} active={props.game} basePath="/" search={props.search} />
      <ClipGrid items={props.items} empty={emptyText(props.search, 'No clips yet. Upload one from the Klipt desktop app.')} />
      <Pager nextCursor={props.nextCursor} basePath="/" game={props.game} search={props.search} />
    </Layout>
  );
}
