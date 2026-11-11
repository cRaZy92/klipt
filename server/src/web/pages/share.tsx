import type { Clip } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import { esc as e } from '../escape.js';
import { Layout } from '../layout.js';
import { skDate } from '../components.js';

function meta(attr: 'property' | 'name', key: string, value: string | number): string {
  return `<meta ${attr}="${key}" content="${e(String(value))}" />`;
}

export function shareHead(clip: Clip): string {
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

export function SharePage(props: { user: AuthUser | null; clip: Clip; canDelete: boolean }): JSX.Element {
  const { clip } = props;
  return (
    <Layout title={clip.title} user={props.user} head={shareHead(clip)}>
      <div class="share" style={`--ar:${clip.width > 0 && clip.height > 0 ? `${clip.width}/${clip.height}` : '16/9'}`}>
        <video
          controls
          {...{ playsinline: true, preload: "metadata" }}
          poster={e(clip.thumbnailUrl)}
          src={e(clip.videoUrl)}
        ></video>
        <aside class="share-info">
        <h1>{e(clip.title)}</h1>
        <p class="meta">
          <a href={`/u/${encodeURIComponent(clip.owner.username)}`}>{e(clip.owner.username)}</a>
          {clip.gameName ? <span> · {e(clip.gameName)}</span> : ''}
          <span> · {skDate(clip.createdAt)}</span>
        </p>
        <div class="actions" data-clip-id={e(clip.id)}>
          <button type="button" class="btn" data-action="copy" data-url={e(clip.shareUrl)}>Copy link</button>
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
