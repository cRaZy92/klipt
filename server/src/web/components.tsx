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

export function formatGB(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

/** Our redirect to the game's Steam header art (the real URL needs a lookup: see steamArt.ts). */
export function steamHeader(appId: number): string {
  return `/steam-art/${appId}`;
}

export function ClipCard(props: { item: ClipWithOwner }): JSX.Element {
  const { clip, owner } = props.item;
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
        </div>
      </div>
    </a>
  );
}

function href(base: string, params: Record<string, string | null | undefined>): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v) q.set(k, v);
  const s = q.toString();
  return s ? `${base}?${s}` : base;
}

export function GameFilter(props: { games: GameCount[]; active?: string; basePath: string }): JSX.Element {
  if (props.games.length === 0) return '';
  return (
    <div class="chips">
      <a class={`chip${props.active ? '' : ' active'}`} href={props.basePath}>
        All
      </a>
      {props.games.map((g) => (
        <a
          class={`chip${props.active === g.gameName ? ' active' : ''}`}
          href={e(href(props.basePath, { game: g.gameName }))}
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

export function Pager(props: { nextCursor: string | null; basePath: string; game?: string }): JSX.Element {
  if (!props.nextCursor) return '';
  return (
    <div class="pager">
      <a class="btn" href={e(href(props.basePath, { game: props.game, cursor: props.nextCursor }))}>
        Older clips
      </a>
    </div>
  );
}

export function ClipGrid(props: { items: ClipWithOwner[]; empty: string }): JSX.Element {
  if (props.items.length === 0) return <p class="empty">{e(props.empty)}</p>;
  return <div class="grid">{props.items.map((item) => <ClipCard item={item} />)}</div>;
}
