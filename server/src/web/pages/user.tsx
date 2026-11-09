import { esc as e } from '../escape.js';
import type { GameCount } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner } from '../../clips/repo.js';
import { ClipGrid, GameFilter, Pager } from '../components.js';
import { Layout } from '../layout.js';

export function UserPage(props: {
  user: AuthUser;
  username: string;
  items: ClipWithOwner[];
  nextCursor: string | null;
  games: GameCount[];
  game?: string;
}): JSX.Element {
  const base = `/u/${encodeURIComponent(props.username)}`;
  return (
    <Layout title={props.username} user={props.user}>
      <h1>{e(props.username)}</h1>
      <GameFilter games={props.games} active={props.game} basePath={base} />
      <ClipGrid items={props.items} empty="No clips here yet." />
      <Pager nextCursor={props.nextCursor} basePath={base} game={props.game} />
    </Layout>
  );
}
