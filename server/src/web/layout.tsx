import Html from '@kitajs/html';
import type { AuthUser } from '../auth/sessions.js';

import { esc as e } from './escape.js';

export function Layout(props: { title: string; user: AuthUser | null; head?: string; children?: Html.Children }): JSX.Element {
  const { title, user } = props;
  return (
    '<!doctype html>' +
    (
      <html lang="en">
        <head>
          <meta charset="utf-8" />
          <meta name="viewport" content="width=device-width, initial-scale=1" />
          <meta name="color-scheme" content="dark" />
          <title>{e(title)} · Klipt</title>
          <link rel="stylesheet" href="/static/app.css" />
          {props.head ?? ''}
        </head>
        <body>
          <header class="topbar">
            <a class="brand" href="/">Klipt</a>
            {user ? (
              <nav>
                <a href="/">Feed</a>
                <a href="/me">My clips</a>
                {user.isAdmin ? <a href="/admin">Admin</a> : ''}
                <span class="who">{e(user.username)}</span>
                <button type="button" class="btn ghost" data-action="logout">Log out</button>
              </nav>
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
