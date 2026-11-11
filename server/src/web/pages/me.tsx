import type { GameCount, SessionInfo } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner } from '../../clips/repo.js';
import { formatDuration, GameFilter, Pager, relativeDate } from '../components.js';
import { Layout } from '../layout.js';

import { esc as e } from '../escape.js';

function MyClipRow(props: { item: ClipWithOwner; baseUrl: string }): JSX.Element {
  const { clip } = props.item;
  return (
    <li class="row" data-clip-id={clip.id}>
      <a class="row-thumb" href={`/c/${clip.id}`}>
        <img src={`/media/${clip.id}.jpg`} alt="" loading="lazy" width="160" height="90" />
        <span class="dur">{formatDuration(clip.durationMs)}</span>
      </a>
      <div class="row-main">
        <a class="row-title" href={`/c/${clip.id}`} data-role="title">{e(clip.title)}</a>
        <div class="meta">
          {clip.gameName ? <span>{e(clip.gameName)}</span> : ''}
          <span>{relativeDate(clip.createdAt)}</span>
          <span class="badge" data-role="visibility">{clip.visibility === 'feed' ? 'In feed' : 'Link only'}</span>
        </div>
        <div class="actions">
          <button type="button" class="btn small" data-action="copy" data-url={e(`${props.baseUrl}/c/${clip.id}`)}>Copy link</button>
          <button type="button" class="btn small" data-action="rename" data-title={e(clip.title)}>Rename</button>
          <button
            type="button"
            class="btn small"
            data-action="visibility"
            data-visibility={clip.visibility}
            title="Feed clips are listed for everyone logged in; link-only clips open only from their link"
          >
            {clip.visibility === 'feed' ? 'Hide from feed' : 'Show in feed'}
          </button>
          <button type="button" class="btn small danger" data-action="delete">Delete</button>
        </div>
      </div>
    </li>
  );
}

export function MePage(props: {
  user: AuthUser;
  items: ClipWithOwner[];
  nextCursor: string | null;
  games: GameCount[];
  game?: string;
  sessions: SessionInfo[];
  baseUrl: string;
}): JSX.Element {
  return (
    <Layout title="My clips" user={props.user}>
      <h1>My clips</h1>
      <GameFilter games={props.games} active={props.game} basePath="/me" />
      {props.items.length === 0 ? (
        <p class="empty">You haven't uploaded any clips yet.</p>
      ) : (
        <ul class="rows">{props.items.map((item) => <MyClipRow item={item} baseUrl={props.baseUrl} />)}</ul>
      )}
      <Pager nextCursor={props.nextCursor} basePath="/me" game={props.game} />
      <h2>Sessions</h2>
      <ul class="rows plain">
        {props.sessions.map((s) => (
          <li class="row session" data-session-id={s.id}>
            <div class="row-main">
              <strong>{e(s.label ?? (s.kind === 'web' ? 'Web' : 'Desktop'))}</strong>
              <span class="badge">{s.kind}</span>
              {s.current ? <span class="badge accent">this device</span> : ''}
              <div class="meta"><span>Last used {relativeDate(new Date(s.lastUsedAt))}</span></div>
            </div>
            {s.current ? '' : <button type="button" class="btn small danger" data-action="revoke-session">Revoke</button>}
          </li>
        ))}
      </ul>
    </Layout>
  );
}
