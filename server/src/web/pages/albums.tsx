import type { AlbumSummary, Clip } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import { esc as e } from '../escape.js';
import { Avatar, Layout } from '../layout.js';
import { formatDuration, relativeDate } from '../components.js';

function meta(attr: 'property' | 'name', key: string, value: string | number): string {
  return `<meta ${attr}="${key}" content="${e(String(value))}" />`;
}

const clipsLabel = (n: number) => (n === 1 ? '1 clip' : `${n} clips`);

/** Link previews (Discord…): the album's title, how many clips, and its first clip's picture. */
export function albumHead(album: AlbumSummary): string {
  return [
    '<meta name="robots" content="noindex" />',
    meta('property', 'og:site_name', 'Klipt'),
    meta('property', 'og:type', 'website'),
    meta('property', 'og:title', album.title),
    meta('property', 'og:description', `${clipsLabel(album.clipCount)} · album by ${album.owner.displayName}`),
    meta('property', 'og:url', album.shareUrl),
    ...(album.coverUrl ? [meta('property', 'og:image', album.coverUrl), meta('name', 'twitter:card', 'summary_large_image')] : []),
    meta('name', 'theme-color', '#7c5cff'),
  ].join('\n');
}

function AlbumCard(props: { album: AlbumSummary }): JSX.Element {
  const { album } = props;
  return (
    <a class="card album-card" href={`/a/${e(album.id)}`}>
      <div class="thumb album-thumb">
        {album.coverUrl ? <img src={e(album.coverUrl)} alt="" loading="lazy" width="320" height="180" /> : <span class="album-empty">Empty</span>}
        <span class="dur">{clipsLabel(album.clipCount)}</span>
      </div>
      <div class="card-body">
        <div class="card-text">
          <div class="card-title">{e(album.title)}</div>
          <div class="meta">
            <span>Changed {relativeDate(new Date(album.updatedAt))}</span>
          </div>
        </div>
      </div>
    </a>
  );
}

/** Your albums, and making a new one. */
export function AlbumsPage(props: { user: AuthUser; albums: AlbumSummary[] }): JSX.Element {
  return (
    <Layout title="Albums" user={props.user} nav="albums">
      <div class="page-head">
        <div>
          <h1>Albums</h1>
          <p class="page-sub">Gather clips, yours or your friends', under one link. Add a clip from its page.</p>
        </div>
        <form class="album-new" data-form="album">
          <input name="title" required maxlength="100" placeholder="Name a new album" aria-label="New album name" />
          <button type="submit" class="btn primary">New album</button>
          <p class="error" role="alert" hidden></p>
        </form>
      </div>
      {props.albums.length === 0 ? (
        <p class="empty">No albums yet. Name one above, then open a clip and press Add to album.</p>
      ) : (
        <div class="grid">{props.albums.map((album) => <AlbumCard album={album} />)}</div>
      )}
    </Layout>
  );
}

/** "12:34 in total" for the album's videos (screenshots don't count). */
function totalLength(clips: Clip[]): string {
  const ms = clips.reduce((sum, c) => sum + (c.kind === 'screenshot' ? 0 : c.durationMs), 0);
  return ms > 0 ? `${formatDuration(ms)} in total` : '';
}

/**
 * The album's page: its name and actions across the top, the clip picked (`?c=`, or the first)
 * playing below with what it is and Previous / Next, and every clip listed beside it (under it on
 * narrow screens). A clip that ends moves on to the next one.
 */
export function AlbumPage(props: { user: AuthUser | null; album: AlbumSummary; clips: Clip[]; current: Clip | null; isOwner: boolean }): JSX.Element {
  const { album, clips, current, isOwner } = props;
  const at = current ? clips.findIndex((c) => c.id === current.id) : -1;
  const prev = at > 0 ? clips[at - 1] : null;
  const next = at >= 0 && at < clips.length - 1 ? clips[at + 1] : null;
  const link = (c: Clip) => `/a/${encodeURIComponent(album.id)}?c=${encodeURIComponent(c.id)}`;
  const ar = current && current.width > 0 && current.height > 0 ? `${current.width}/${current.height}` : '16/9';
  const total = totalLength(clips);
  return (
    <Layout title={album.title} user={props.user} head={albumHead(album)}>
      <div class="album-page" data-album-id={e(album.id)} style={`--ar:${ar}`}>
        <header class="album-head">
          <div class="album-head-text">
            <p class="album-kicker">Album</p>
            <h1 data-role="album-title">{e(album.title)}</h1>
            <p class="album-byline">
              <a class="album-owner" href={`/u/${encodeURIComponent(album.owner.username)}`}>
                <Avatar name={album.owner.displayName} url={album.owner.avatarUrl} frame={album.owner.frame} size="sm" />
                <strong>{e(album.owner.displayName)}</strong>
              </a>
              <span>{clipsLabel(clips.length)}</span>
              {total ? <span>{total}</span> : ''}
              <span>Changed {relativeDate(new Date(album.updatedAt))}</span>
            </p>
          </div>
          <div class="album-actions">
            <button type="button" class="btn primary" data-action="copy" data-url={e(album.shareUrl)}>
              Copy album link
            </button>
            {isOwner ? (
              <>
                <button type="button" class="btn" data-action="album-rename" data-title={e(album.title)}>Rename</button>
                <button type="button" class="btn ghost danger" data-action="album-delete">Delete</button>
              </>
            ) : (
              ''
            )}
          </div>
        </header>

        <div class="album-body">
          <section class="album-stage">
            {current ? (
              current.kind === 'screenshot' ? (
                <img class="album-media shot" src={e(current.videoUrl)} alt={e(current.title)} data-view-url={`/api/clips/${e(current.id)}/view`} />
              ) : (
                <video
                  class="album-media"
                  controls
                  {...{ playsinline: true, preload: 'metadata' }}
                  poster={e(current.thumbnailUrl)}
                  src={e(current.videoUrl)}
                  data-view-url={`/api/clips/${e(current.id)}/view`}
                  data-next-url={next ? e(link(next)) : undefined}
                ></video>
              )
            ) : (
              <div class="album-placeholder">
                <p>No clips in this album yet.</p>
                {isOwner ? <p class="muted">Open a clip and press Add to album.</p> : ''}
              </div>
            )}
            {current ? (
              <div class="album-now">
                <div class="album-now-text">
                  <p class="album-now-title"><a href={`/c/${e(current.id)}`}>{e(current.title)}</a></p>
                  <p class="meta">
                    <span>{at + 1} of {clips.length}</span>
                    <a class="person-link" href={`/u/${encodeURIComponent(current.owner.username)}`}>{e(current.owner.displayName)}</a>
                    {current.gameName ? <span>{e(current.gameName)}</span> : ''}
                  </p>
                </div>
                <nav class="album-steps" aria-label="Previous and next clip">
                  {prev ? <a class="btn" href={e(link(prev))} rel="prev">‹ Previous</a> : <span class="btn" aria-disabled="true">‹ Previous</span>}
                  {next ? <a class="btn" href={e(link(next))} rel="next">Next ›</a> : <span class="btn" aria-disabled="true">Next ›</span>}
                </nav>
              </div>
            ) : (
              ''
            )}
          </section>

          {clips.length > 0 ? (
            <aside class="album-playlist" aria-label="Clips in this album">
              <div class="album-playlist-head">
                <h2>Clips</h2>
                <span class="muted">{clipsLabel(clips.length)}</span>
              </div>
              <ol class="album-list">
                {clips.map((c, i) => (
                  <li class={c.id === current?.id ? 'current' : ''} data-album-clip={e(c.id)}>
                    <a class="album-item" href={e(link(c))} aria-current={c.id === current?.id ? 'true' : undefined}>
                      <span class="album-num">{i + 1}</span>
                      <span class="album-item-thumb">
                        <img src={e(c.thumbnailUrl)} alt="" loading="lazy" width="128" height="72" />
                        <span class="dur">{c.kind === 'screenshot' ? 'Shot' : formatDuration(c.durationMs)}</span>
                      </span>
                      <span class="album-item-text">
                        <span class="album-item-title" title={e(c.title)}>{e(c.title)}</span>
                        <span class="meta">
                          <span>{e(c.owner.displayName)}</span>
                          {c.gameName ? <span>{e(c.gameName)}</span> : ''}
                        </span>
                      </span>
                    </a>
                    {isOwner ? (
                      <span class="album-item-tools">
                        <button type="button" class="icon-btn" data-action="album-move" data-dir="-1" aria-label="Move up" title="Move up" disabled={i === 0}>↑</button>
                        <button type="button" class="icon-btn" data-action="album-move" data-dir="1" aria-label="Move down" title="Move down" disabled={i === clips.length - 1}>↓</button>
                        <button type="button" class="icon-btn" data-action="album-remove" aria-label="Remove from the album" title="Remove from the album">✕</button>
                      </span>
                    ) : (
                      ''
                    )}
                  </li>
                ))}
              </ol>
            </aside>
          ) : (
            ''
          )}
        </div>
      </div>
    </Layout>
  );
}

/** On a clip's page: the list of your albums to put it in, filled in by app.js when opened. */
export function AlbumPicker(props: { clipId: string }): JSX.Element {
  return (
    <dialog class="album-picker" data-clip-id={e(props.clipId)} aria-labelledby="album-picker-title">
      <form method="dialog" class="album-picker-head">
        <h2 id="album-picker-title">Add to album</h2>
        <button class="icon-btn" aria-label="Close">✕</button>
      </form>
      <ul class="album-choices" data-role="album-choices">
        <li class="muted">Loading your albums…</li>
      </ul>
      <form class="album-picker-new" data-role="album-new">
        <input name="title" required maxlength="100" placeholder="New album" aria-label="New album name" />
        <button type="submit" class="btn small primary">Create and add</button>
      </form>
    </dialog>
  );
}
