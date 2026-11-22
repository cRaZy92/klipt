import Html from '@kitajs/html';
import type { AuthUser } from '../auth/sessions.js';

import { esc as e } from './escape.js';

/** The top bar's sections; the page passes its own so the bar can mark it. */
export type NavSection = 'feed' | 'me' | 'albums' | 'get-app' | 'admin' | 'account';

/** The app icon: a play triangle between two clip brackets (desktop/src-tauri/icons/source.svg). */
export function LogoMark(props: { size?: number }): JSX.Element {
  const size = String(props.size ?? 28);
  return (
    <svg class="logo-mark" width={size} height={size} viewBox="64 64 896 896" aria-hidden="true">
      <rect x="64" y="64" width="896" height="896" rx="200" fill="#7c5cff" />
      <path
        d="M300 300h-60v424h60M724 300h60v424h-60"
        fill="none"
        stroke="#fff"
        stroke-width="64"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path d="M440 380v264l220-132z" fill="#fff" />
    </svg>
  );
}

/** A friend's initial on a colour picked from their name, so each person keeps the same one. */
export function Avatar(props: { name: string; size?: 'sm' | 'md' | 'lg' }): JSX.Element {
  let hash = 0;
  for (const ch of props.name.toLowerCase()) hash = (hash * 31 + ch.codePointAt(0)!) >>> 0;
  const initial = [...props.name][0]?.toUpperCase() ?? '?';
  return (
    <span class={`avatar ${props.size ?? 'sm'}`} style={`--h:${hash % 360}`} aria-hidden="true">
      {e(initial)}
    </span>
  );
}

export function Layout(props: {
  title: string;
  user: AuthUser | null;
  head?: string;
  nav?: NavSection;
  children?: Html.Children;
}): JSX.Element {
  const { title, user } = props;
  const link = (section: NavSection, href: string, label: string) => (
    <a href={href} aria-current={props.nav === section ? 'page' : undefined}>
      {label}
    </a>
  );
  return (
    '<!doctype html>' +
    (
      <html lang="en">
        <head>
          <meta charset="utf-8" />
          <meta name="viewport" content="width=device-width, initial-scale=1" />
          <meta name="color-scheme" content="dark" />
          <meta name="theme-color" content="#0b0a10" />
          <title>{e(title)} · Klipt</title>
          <link rel="preload" href="/static/fonts/bricolage-grotesque-latin-wght-normal.woff2" as="font" type="font/woff2" crossorigin="" />
          <link rel="stylesheet" href="/static/app.css" />
          {props.head ?? ''}
        </head>
        <body>
          <header class="topbar">
            <a class="brand" href="/" aria-label="Klipt, go to the feed">
              <LogoMark />
              <span>Klipt</span>
            </a>
            {user ? (
              <>
                <nav class="nav" aria-label="Main">
                  {link('feed', '/', 'Feed')}
                  {link('me', '/me', 'My clips')}
                  {link('albums', '/albums', 'Albums')}
                  {link('get-app', '/get-app', 'Get the app')}
                  {user.isAdmin ? link('admin', '/admin', 'Admin') : ''}
                </nav>
                <div class="account">
                  <a class="who" href={`/u/${encodeURIComponent(user.username)}`} title="Your profile">
                    <Avatar name={user.displayName} />
                    <span>{e(user.displayName)}</span>
                  </a>
                  <a
                    class="btn ghost small"
                    href="/account"
                    aria-current={props.nav === 'account' ? 'page' : undefined}
                    title="Name, password, devices and feedback"
                  >
                    Account
                  </a>
                  <button type="button" class="btn ghost small" data-action="logout">Log out</button>
                </div>
              </>
            ) : (
              ''
            )}
          </header>
          <main class="container">{props.children}</main>
          <div id="toast" class="toast" role="status" hidden></div>
          <script src="/static/app.js" defer></script>
        </body>
      </html>
    )
  );
}
