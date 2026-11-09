import type { GameCount } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner } from '../../clips/repo.js';
import { ClipGrid, GameFilter, Pager } from '../components.js';
import { Layout } from '../layout.js';

export function FeedPage(props: {
  user: AuthUser;
  items: ClipWithOwner[];
  nextCursor: string | null;
  games: GameCount[];
  game?: string;
}): JSX.Element {
  return (
    <Layout title="Feed" user={props.user}>
      <h1>Feed</h1>
      <GameFilter games={props.games} active={props.game} basePath="/" />
      <ClipGrid items={props.items} empty="No clips yet. Upload one from the Klipt desktop app." />
      <Pager nextCursor={props.nextCursor} basePath="/" game={props.game} />
    </Layout>
  );
}
