import { esc as e } from '../escape.js';
import { Layout, LogoMark } from '../layout.js';
import { LINUX_INSTALL } from './getApp.js';

/** What the page shows. Images live in `server/public/showcase/`. */
const PILLARS = [
  {
    id: 'record',
    tag: 'Record',
    title: 'Your last 30 seconds, on a key',
    body: 'Klipt records in the background. Press Alt+` after a great moment and it keeps it. Screenshots and full recordings too, and it knows which game you were playing.',
  },
  {
    id: 'edit',
    tag: 'Edit',
    title: 'A real editor, not a trim slider',
    body: 'Trim, cut out the boring middle, zoom in on the kill, add subtitles, blur a name, pick the thumbnail. Combine clips into one.',
  },
  {
    id: 'share',
    tag: 'Share',
    title: 'One link that plays in Discord',
    body: 'Upload and paste. The clip plays right in the chat. Keep it link only, show it to friends, or make it public.',
  },
  {
    id: 'friends',
    tag: 'Friends',
    title: 'A feed of just your friends',
    body: 'See what your friends clipped, tag them in yours, and put the best ones in albums. Send a link to bring someone in.',
  },
  {
    id: 'looks',
    tag: 'Make it yours',
    title: 'Badges worth chasing',
    body: 'A profile picture, banners, frames and themes, unlocked by 32 achievements.',
  },
] as const;

/**
 * Klipt vs Medal: only rows Medal's own pages back up (checked 2026-11-27).
 * - Ads: https://medal.tv/premium lists "Ad free" for Premium ("Ads help keep Medal free").
 * - Watermark: https://medal.tv/premium, "Removable watermarks" ("Download your clips without the watermark").
 * - Upload length: https://medal.tv/premium, "Upload clips of any length instead of maxxing out at 20 minutes."
 * - Linux: https://medal.tv/ offers downloads for Windows, iOS and Android only.
 */
export const COMPARISON: { what: string; klipt: string; medal: string | null }[] = [
  { what: 'Ads', klipt: 'None', medal: 'Unless you pay' },
  { what: 'Watermark on downloads', klipt: 'None', medal: 'Unless you pay' },
  { what: 'Upload length', klipt: 'No time cap', medal: '20 minutes free' },
  { what: 'Linux app', klipt: 'Yes', medal: 'No' },
];

const VIDEO_POSTER = '/static/showcase/hero.jpg';
const VIDEO = '/static/showcase/hero.mp4';

/** klipt.adamko.tech for visitors who aren't logged in. */
export function HomePage(props: { referrer: string | null; referralInvalid: boolean }): JSX.Element {
  const withMedal = COMPARISON.some((r) => r.medal !== null);
  return (
    <Layout title="Clips without bullshit" user={null}>
      <div class="home">
        {props.referrer ? (
          <p class="home-banner">
            <strong>{e(props.referrer)}</strong> invited you. <a href="/signup">Sign up</a> and you'll be friends.
          </p>
        ) : props.referralInvalid ? (
          <p class="home-banner">That invite link was already used or expired. You can still sign up.</p>
        ) : (
          ''
        )}

        <section class="home-hero">
          <div class="home-hero-text">
            <p class="home-kicker">Free · Windows and Linux</p>
            <h1>Clips without bullshit.</h1>
            <p class="home-lead">
              Records your games in the background, saves the last 30 seconds on a key, edits and shares with one link.
              No ads, no watermark, no paywall.
            </p>
            <div class="home-cta">
              <a class="btn primary big" href="/get-app/windows">Download for Windows</a>
              <a class="btn big" href="/signup">Create an account</a>
            </div>
            <p class="muted home-small">
              On Arch Linux? <a href="#download">There's a package</a>.
            </p>
          </div>
          <div class="home-hero-video">
            <span class="bracket left" aria-hidden="true"></span>
            <video src={VIDEO} poster={VIDEO_POSTER} autoplay muted loop {...{ playsinline: true, preload: 'auto' }} aria-label="A clip being saved, trimmed and shared"></video>
            <span class="bracket right" aria-hidden="true"></span>
          </div>
        </section>

        <section class="home-pillars" id="features" aria-label="Features">
          {PILLARS.map((p, i) => (
            <article class={`home-pillar${i === 0 ? ' wide' : ''}`}>
              <img src={`/static/showcase/${p.id}.webp`} alt="" loading="lazy" width="1600" height="1000" />
              <div class="home-pillar-text">
                <p class="home-tag">{p.tag}</p>
                <h2>{p.title}</h2>
                <p class="muted">{p.body}</p>
              </div>
            </article>
          ))}
        </section>

        <section class="home-compare" aria-labelledby="compare-title">
          <h2 id="compare-title">{withMedal ? 'Klipt and Medal' : 'What you won’t find here'}</h2>
          <table>
            <thead>
              <tr>
                <th scope="col"></th>
                <th scope="col">Klipt</th>
                {withMedal ? <th scope="col">Medal</th> : ''}
              </tr>
            </thead>
            <tbody>
              {COMPARISON.map((r) => (
                <tr>
                  <th scope="row">{e(r.what)}</th>
                  <td class="yes">{e(r.klipt)}</td>
                  {withMedal ? <td>{e(r.medal ?? '')}</td> : ''}
                </tr>
              ))}
            </tbody>
          </table>
        </section>

        <section class="home-download" id="download">
          <LogoMark size={52} />
          <h2>Get Klipt</h2>
          <p class="muted">Free. Make an account, install the app, log in. That's it.</p>
          <div class="home-cta">
            <a class="btn primary big" href="/get-app/windows">Download for Windows</a>
            <a class="btn big" href="/signup">Create an account</a>
          </div>
          <details class="linux">
            <summary>On Arch Linux?</summary>
            <p>Run this in a terminal. It builds and installs the Klipt package, and asks for your password once:</p>
            <p class="copy-line">
              <code>{e(LINUX_INSTALL)}</code>
              <button type="button" class="btn small" data-action="copy" data-url={e(LINUX_INSTALL)}>
                Copy
              </button>
            </p>
          </details>
        </section>

        <footer class="home-footer muted">
          <a href="https://github.com/cRaZy92/klipt">Open source on GitHub</a>
          <span aria-hidden="true">·</span>
          <a href="/login">Log in</a>
        </footer>
      </div>
    </Layout>
  );
}
