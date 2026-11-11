import type { GameCount } from '@klipt/shared';
import type { ClipWithOwner } from '../clips/repo.js';

import { esc as e } from './escape.js';

export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

const SK_DATE = new Intl.DateTimeFormat('sk-SK', {
  timeZone: 'Europe/Bratislava',
  day: 'numeric',
  month: 'numeric',
  year: 'numeric',
});

/** Slovak date, e.g. "10. 11. 2026" (in Slovak time, so late-night clips keep their day). */
export function skDate(date: Date | string): string {
  const parts = SK_DATE.formatToParts(new Date(date));
  const part = (type: string) => parts.find((p) => p.type === type)?.value ?? '';
  return `${part('day')}. ${part('month')}. ${part('year')}`;
}

export function relativeDate(date: Date, now = new Date()): string {
  const sec = Math.max(0, Math.floor((now.getTime() - date.getTime()) / 1000));
  const units: [number, string][] = [
    [31536000, 'year'],
    [2592000, 'month'],
    [604800, 'week'],
    [86400, 'day'],
    [3600, 'hour'],
    [60, 'minute'],
  ];
  for (const [size, name] of units) {
    if (sec >= size) {
      const n = Math.floor(sec / size);
      return `${n} ${name}${n === 1 ? '' : 's'} ago`;
    }
  }
  return 'just now';
}

/** 0.4 MB, 9.5 MB, 340 MB, 1.5 GB, 12 GB: one decimal below 10, a trailing ".0" dropped. */
export function formatSize(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  const [value, unit] = gb >= 1 ? [gb, 'GB'] : [bytes / 1024 ** 2, 'MB'];
  const shown = value >= 10 ? String(Math.round(value)) : (bytes > 0 ? Math.max(value, 0.1) : 0).toFixed(1).replace(/\.0$/, '');
  return `${shown} ${unit}`;
}

export function formatGB(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

/** Our redirect to the game's Steam header art (the real URL needs a lookup: see steamArt.ts). */
export function steamHeader(appId: number): string {
  return `/steam-art/${appId}`;
}

/** "1 view", "12 views". */
export function viewsLabel(views: number): string {
  return views === 1 ? '1 view' : `${views} views`;
}

export function ClipCard(props: { item: ClipWithOwner }): JSX.Element {
  const { clip, owner, views } = props.item;
  return (
    <a class="card" href={`/c/${clip.id}`}>
      <div class="thumb">
        <img src={`/media/${clip.id}.jpg`} alt="" loading="lazy" width="320" height="180" />
        <span class="dur">{formatDuration(clip.durationMs)}</span>
      </div>
      <div class="card-body">
        <div class="card-title">{e(clip.title)}</div>
        <div class="meta">
          {clip.gameName ? <span>{e(clip.gameName)}</span> : ''}
          <span>{e(owner.username)}</span>
          <span>{relativeDate(clip.createdAt)}</span>
          {views > 0 ? <span data-role="views">{viewsLabel(views)}</span> : ''}
        </div>
      </div>
    </a>
  );
}

function href(base: string, params: Record<string, string | null | undefined>): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v) q.set(k, v);
  const s = q.toString();
  return s ? `${base}${base.includes('?') ? '&' : '?'}${s}` : base;
}

/** A search box for the list at `basePath`; the game filter is kept. */
export function SearchBox(props: { basePath: string; search?: string; game?: string; withMe?: boolean }): JSX.Element {
  return (
    <form class="search" method="get" action={e(props.basePath)} role="search">
      <input type="search" name="q" value={e(props.search ?? '')} placeholder="Search by title, game or friend" aria-label="Search clips" maxlength="100" />
      {props.game ? <input type="hidden" name="game" value={e(props.game)} /> : ''}
      {props.withMe ? <input type="hidden" name="with" value="me" /> : ''}
      <button type="submit" class="btn">Search</button>
    </form>
  );
}

/** The empty-list text: a search that found nothing says so. */
export function emptyText(search: string | undefined, otherwise: string): string {
  return search ? `No clips match “${search}”.` : otherwise;
}

export function GameFilter(props: { games: GameCount[]; active?: string; basePath: string; search?: string }): JSX.Element {
  if (props.games.length === 0) return '';
  return (
    <div class="chips">
      <a class={`chip${props.active ? '' : ' active'}`} href={e(href(props.basePath, { q: props.search }))}>
        All
      </a>
      {props.games.map((g) => (
        <a
          class={`chip${props.active === g.gameName ? ' active' : ''}`}
          href={e(href(props.basePath, { game: g.gameName, q: props.search }))}
          title={e(g.gameName)}
        >
          {g.steamAppId ? <img class="chip-art" src={steamHeader(g.steamAppId)} alt="" loading="lazy" /> : ''}
          <span class="chip-name">{e(g.gameName)}</span>
          <span class="chip-count">{String(g.count)}</span>
        </a>
      ))}
    </div>
  );
}

/** The feed's "I'm in it" filter: clips you're tagged in. */
export function WithMeChip(props: { active: boolean }): JSX.Element {
  return (
    <div class="chips">
      <a class={`chip${props.active ? ' active' : ''}`} href={props.active ? '/' : '/?with=me'} title="Clips your friends tagged you in">
        I'm in it
      </a>
    </div>
  );
}

export function Pager(props: { nextCursor: string | null; basePath: string; game?: string; search?: string }): JSX.Element {
  if (!props.nextCursor) return '';
  return (
    <div class="pager">
      <a class="btn" href={e(href(props.basePath, { game: props.game, q: props.search, cursor: props.nextCursor }))}>
        Older clips
      </a>
    </div>
  );
}

export function ClipGrid(props: { items: ClipWithOwner[]; empty: string }): JSX.Element {
  if (props.items.length === 0) return <p class="empty">{e(props.empty)}</p>;
  return <div class="grid">{props.items.map((item) => <ClipCard item={item} />)}</div>;
}
