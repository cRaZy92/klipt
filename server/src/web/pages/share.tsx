import type { Clip } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import { esc as e } from '../escape.js';
import { Layout } from '../layout.js';
import { skDate, viewsLabel } from '../components.js';

function meta(attr: 'property' | 'name', key: string, value: string | number): string {
  return `<meta ${attr}="${key}" content="${e(String(value))}" />`;
}

export function shareHead(clip: Clip): string {
  if (clip.kind === 'screenshot') {
    return [
      '<meta name="robots" content="noindex" />',
      meta('property', 'og:site_name', 'Klipt'),
      meta('property', 'og:type', 'website'),
      meta('property', 'og:title', clip.title),
      meta('property', 'og:description', clip.gameName ? `${clip.gameName} · screenshot by ${clip.owner.username}` : clip.owner.username),
      meta('property', 'og:url', clip.shareUrl),
      meta('property', 'og:image', clip.videoUrl),
      meta('property', 'og:image:width', clip.width),
      meta('property', 'og:image:height', clip.height),
      meta('name', 'twitter:card', 'summary_large_image'),
      meta('name', 'theme-color', '#7c5cff'),
    ].join('\n');
  }
  const description = clip.gameName ? `${clip.gameName} · clipped by ${clip.owner.username}` : clip.owner.username;
  return [
    '<meta name="robots" content="noindex" />',
    meta('property', 'og:site_name', 'Klipt'),
    meta('property', 'og:type', 'video.other'),
    meta('property', 'og:title', clip.title),
    meta('property', 'og:description', description),
    meta('property', 'og:url', clip.shareUrl),
    meta('property', 'og:image', clip.thumbnailUrl),
    meta('property', 'og:video', clip.videoUrl),
    meta('property', 'og:video:secure_url', clip.videoUrl),
    meta('property', 'og:video:type', 'video/mp4'),
    meta('property', 'og:video:width', clip.width),
    meta('property', 'og:video:height', clip.height),
    meta('name', 'twitter:card', 'player'),
    meta('name', 'theme-color', '#7c5cff'),
  ].join('\n');
}

export function SharePage(props: {
  user: AuthUser | null;
  clip: Clip;
  canDelete: boolean;
  /** Friends who watched it; only given to the owner. */
  viewers?: string[];
}): JSX.Element {
  const { clip } = props;
  return (
    <Layout title={clip.title} user={props.user} head={shareHead(clip)}>
      <div class="share" style={`--ar:${clip.width > 0 && clip.height > 0 ? `${clip.width}/${clip.height}` : '16/9'}`}>
        {clip.kind === 'screenshot' ? (
          <img class="shot" src={e(clip.videoUrl)} alt={e(clip.title)} data-view-url={`/api/clips/${e(clip.id)}/view`} />
        ) : (
        <video
            controls
            {...{ playsinline: true, preload: "metadata" }}
            poster={e(clip.thumbnailUrl)}
            src={e(clip.videoUrl)}
            data-view-url={`/api/clips/${e(clip.id)}/view`}
          ></video>
        )}
        <aside class="share-info">
        <h1>{e(clip.title)}</h1>
        <p class="meta">
          <a href={`/u/${encodeURIComponent(clip.owner.username)}`}>{e(clip.owner.username)}</a>
          {clip.gameName ? <span> · {e(clip.gameName)}</span> : ''}
          <span> · {skDate(clip.createdAt)}</span>
          <span data-role="views"> · {viewsLabel(clip.views)}</span>
        </p>
        {clip.tagged.length > 0 ? (
          <p class="meta" data-role="tagged">
            With {clip.tagged.map((name, i) => `${i > 0 ? ', ' : ''}<a href="/u/${encodeURIComponent(name)}">${e(name)}</a>`).join('')}
          </p>
        ) : (
          ''
        )}
        {props.viewers ? (
          <p class="meta seen-by" data-role="seen-by">
            {props.viewers.length > 0 ? `Seen by ${props.viewers.map((v) => e(v)).join(', ')}` : 'No friends have watched it yet.'}
          </p>
        ) : (
          ''
        )}
        <div class="actions" data-clip-id={e(clip.id)}>
          <button type="button" class="btn" data-action="copy" data-url={e(clip.shareUrl)}>Copy link</button>
          {props.user ? (
            <button type="button" class={`btn fav${clip.favorite ? ' on' : ''}`} data-action="favorite" aria-pressed={String(clip.favorite)}>
              {clip.favorite ? '★ Favorite' : '☆ Favorite'}
            </button>
          ) : (
            ''
          )}
          {props.viewers ? (
            <button type="button" class="btn" data-action="tag" data-tagged={e(clip.tagged.join(', '))}>Tag friends</button>
          ) : (
            ''
          )}
          <a class="btn" href={e(`${clip.videoUrl}?download=1`)} download="">Download</a>
          {props.canDelete ? (
            <button type="button" class="btn danger" data-action="delete" data-redirect="/">Delete</button>
          ) : (
            ''
          )}
        </div>
        </aside>
      </div>
    </Layout>
  );
}
