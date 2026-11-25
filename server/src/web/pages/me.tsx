import type { GameCount, MyStorage } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import type { ClipWithOwner } from '../../clips/repo.js';
import { thumbnailUrl } from '../../clips/routes.js';
import { emptyText, formatDuration, formatSize, GameFilter, Pager, ProcessingPill, relativeDate, SearchBox, viewsLabel } from '../components.js';
import { Layout } from '../layout.js';

import { esc as e } from '../escape.js';

function MyClipRow(props: { item: ClipWithOwner; baseUrl: string }): JSX.Element {
  const { clip, views } = props.item;
  return (
    <li class="row" data-clip-id={clip.id}>
      <a class="row-thumb" href={`/c/${clip.id}`}>
        <img src={thumbnailUrl(clip, '')} alt="" loading="lazy" width="160" height="90" />
        <ProcessingPill state={clip.transcodeState} />
        <span class="dur">{clip.kind === 'screenshot' ? 'Screenshot' : formatDuration(clip.durationMs)}</span>
      </a>
      <div class="row-main">
        <a class="row-title" href={`/c/${clip.id}`} data-role="title">{e(clip.title)}</a>
        <div class="meta">
          {clip.gameName ? <span>{e(clip.gameName)}</span> : ''}
          <span>{relativeDate(clip.createdAt)}</span>
          <span data-role="views">{viewsLabel(views)}</span>
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
  search?: string;
  storage: MyStorage;
  baseUrl: string;
}): JSX.Element {
  const { storage } = props;
  return (
    <Layout title="My clips" user={props.user} nav="me">
      <div class="page-head">
        <div>
          <h1>My clips</h1>
          {storage.clips > 0 ? (
            <p class="page-sub" data-role="storage">
              {storage.clips === 1 ? '1 clip' : `${storage.clips} clips`}, {formatSize(storage.bytes)} on the server
            </p>
          ) : (
            ''
          )}
        </div>
        <SearchBox basePath="/me" search={props.search} game={props.game} />
      </div>
      <GameFilter games={props.games} active={props.game} basePath="/me" search={props.search} />
      {props.items.length === 0 ? (
        <p class="empty">{e(emptyText(props.search, "You haven't uploaded any clips yet."))}</p>
      ) : (
        <ul class="rows">{props.items.map((item) => <MyClipRow item={item} baseUrl={props.baseUrl} />)}</ul>
      )}
      <Pager nextCursor={props.nextCursor} basePath="/me" game={props.game} search={props.search} />
    </Layout>
  );
}
