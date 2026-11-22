import type { Clip } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import { esc as e } from '../escape.js';
import { Avatar, Layout } from '../layout.js';
import { skDate, viewsLabel } from '../components.js';
import { AlbumPicker } from './albums.js';

function meta(attr: 'property' | 'name', key: string, value: string | number): string {
  return `<meta ${attr}="${key}" content="${e(String(value))}" />`;
}

/** The small copy of the video that link embeds use instead of the full one (see clips/embed.ts). */
export interface ShareEmbed {
  url: string;
  width: number;
  height: number;
}

export function shareHead(clip: Clip, embed: ShareEmbed | null = null): string {
  if (clip.kind === 'screenshot') {
    return [
      '<meta name="robots" content="noindex" />',
      meta('property', 'og:site_name', 'Klipt'),
      meta('property', 'og:type', 'website'),
      meta('property', 'og:title', clip.title),
      meta('property', 'og:description', clip.gameName ? `${clip.gameName} · screenshot by ${clip.owner.displayName}` : clip.owner.displayName),
      meta('property', 'og:url', clip.shareUrl),
      meta('property', 'og:image', clip.videoUrl),
      meta('property', 'og:image:width', clip.width),
      meta('property', 'og:image:height', clip.height),
      meta('name', 'twitter:card', 'summary_large_image'),
      meta('name', 'theme-color', '#7c5cff'),
    ].join('\n');
  }
  const description = clip.gameName ? `${clip.gameName} · clipped by ${clip.owner.displayName}` : clip.owner.displayName;
  const video = embed ?? { url: clip.videoUrl, width: clip.width, height: clip.height };
  return [
    '<meta name="robots" content="noindex" />',
    meta('property', 'og:site_name', 'Klipt'),
    meta('property', 'og:type', 'video.other'),
    meta('property', 'og:title', clip.title),
    meta('property', 'og:description', description),
    meta('property', 'og:url', clip.shareUrl),
    meta('property', 'og:image', clip.thumbnailUrl),
    meta('property', 'og:video', video.url),
    meta('property', 'og:video:secure_url', video.url),
    meta('property', 'og:video:type', 'video/mp4'),
    meta('property', 'og:video:width', video.width),
    meta('property', 'og:video:height', video.height),
    meta('name', 'twitter:card', 'player'),
    meta('name', 'theme-color', '#7c5cff'),
  ].join('\n');
}

/** Beside the clip for logged-out visitors: make an account right here, invited by the clip's owner. */
function JoinCard(props: { clip: Clip }): JSX.Element {
  const { clip } = props;
  const here = `/c/${encodeURIComponent(clip.id)}`;
  return (
    <section class="join-card" aria-labelledby="join-title">
      <h2 id="join-title">Create an account</h2>
      {/* A new account first gets the install guide; its Continue button comes back here. */}
      <form data-form="register" data-next={e(`/get-app?welcome=1&next=${encodeURIComponent(here)}`)} class="form">
        <input type="hidden" name="clipId" value={e(clip.id)} />
        <label>Username<input name="username" autocomplete="username" required minlength="3" maxlength="24" pattern="[A-Za-z0-9_]{3,24}" title="3-24 letters, numbers or underscores" /></label>
        <label>Password<input name="password" type="password" autocomplete="new-password" required minlength="8" maxlength="1024" /></label>
        <p class="error" role="alert" hidden></p>
        <button class="btn primary" type="submit">Create account</button>
      </form>
      <p class="join-login">
        Already have an account? <a href={e(`/login?next=${encodeURIComponent(here)}`)}>Log in</a>
      </p>
    </section>
  );
}

/** A small line icon for a button (24×24 stroke paths). */
function Icon(props: { d: string }): JSX.Element {
  return (
    <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d={props.d} />
    </svg>
  );
}

/** Who's in the clip, and for the owner who watched it, with tagging right where the names are. */
function People(props: { clip: Clip; viewers?: string[] }): JSX.Element {
  const { clip, viewers } = props;
  const owner = !!viewers;
  const tag = (label: string) => (
    <button type="button" class="link-btn" data-action="tag" data-tagged={e(clip.tagged.join(', '))}>
      {label}
    </button>
  );
  if (clip.tagged.length === 0 && !owner) return '';
  return (
    <div class="share-people">
      {clip.tagged.length > 0 ? (
        <p class="meta">
          <span data-role="tagged">
            With {clip.tagged.map((name, i) => `${i > 0 ? ', ' : ''}<a href="/u/${encodeURIComponent(name)}">${e(clip.taggedNames[i] ?? name)}</a>`).join('')}
          </span>
          {owner ? tag('Edit') : ''}
        </p>
      ) : (
        <p class="meta">{tag('Tag friends in this clip')}</p>
      )}
      {viewers ? (
        <p class="meta seen-by" data-role="seen-by">
          {viewers.length > 0 ? `Seen by ${viewers.map((v) => e(v)).join(', ')}` : 'No friends have watched it yet.'}
        </p>
      ) : (
        ''
      )}
    </div>
  );
}

export function SharePage(props: {
  user: AuthUser | null;
  clip: Clip;
  /** The clip's embed copy, when it has one. */
  embed?: ShareEmbed | null;
  canDelete: boolean;
  /** Friends who watched it; only given to the owner. */
  viewers?: string[];
  /** Offer a logged-out visitor an account, invited by the clip's owner. */
  signup?: boolean;
}): JSX.Element {
  const { clip } = props;
  return (
    <Layout title={clip.title} user={props.user} head={shareHead(clip, props.embed ?? null)}>
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
        <aside class="share-info" data-clip-id={e(clip.id)}>
          {clip.gameName ? <p class="share-game">{e(clip.gameName)}</p> : ''}
          <h1>{e(clip.title)}</h1>
          <a class="share-owner" href={`/u/${encodeURIComponent(clip.owner.username)}`}>
            <Avatar name={clip.owner.displayName} size="md" />
            <span>
              <strong>{e(clip.owner.displayName)}</strong>
              <span class="meta">
                <span>{skDate(clip.createdAt)}</span>
                <span data-role="views">{viewsLabel(clip.views)}</span>
              </span>
            </span>
          </a>
          <People clip={clip} viewers={props.viewers} />
          <div class="share-main">
            {/* Logged out, signing up is the main thing on the page, so the link button steps back. */}
            <button type="button" class={props.user ? 'btn primary' : 'btn'} data-action="copy" data-url={e(clip.shareUrl)}>
              <Icon d="M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1 1M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1-1" />
              Copy link
            </button>
            {props.user ? (
              <button type="button" class={`btn fav${clip.favorite ? ' on' : ''}`} data-action="favorite" aria-pressed={String(clip.favorite)}>
                {clip.favorite ? '★ Favorite' : '☆ Favorite'}
              </button>
            ) : (
              ''
            )}
            {props.user ? (
              <button type="button" class="btn" data-action="album-pick" title="Put this clip in one of your albums">
                <Icon d="M4 6h16M4 12h10M4 18h7M18 14v6M15 17h6" />
                Add to album
              </button>
            ) : (
              ''
            )}
          </div>
          <div class="share-more">
            <a class="btn ghost small" href={e(`${clip.videoUrl}?download=1`)} download="">
              <Icon d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 20h14" />
              Download
            </a>
            {props.canDelete ? (
              <button type="button" class="btn ghost small danger" data-action="delete" data-redirect="/">
                <Icon d="M5 7h14M10 7V5h4v2M7 7l1 13h8l1-13" />
                Delete
              </button>
            ) : (
              ''
            )}
          </div>
          {props.signup && !props.user ? <JoinCard clip={clip} /> : ''}
        </aside>
        {props.user ? <AlbumPicker clipId={clip.id} /> : ''}
      </div>
    </Layout>
  );
}
