import type { GameCount } from '@klipt/shared';
import type { ClipWithOwner } from '../clips/repo.js';

import { esc as e } from './escape.js';

export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
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

export function steamHeader(appId: number): string {
  return `https://cdn.cloudflare.steamstatic.com/steam/apps/${appId}/header.jpg`;
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
          href={href(props.basePath, { game: g.gameName })}
          title={g.gameName}
        >
          {g.steamAppId ? (
            <img src={steamHeader(g.steamAppId)} alt={g.gameName} loading="lazy" height="28" />
          ) : (
            e(g.gameName)
          )}
        </a>
      ))}
    </div>
  );
}

export function Pager(props: { nextCursor: string | null; basePath: string; game?: string }): JSX.Element {
  if (!props.nextCursor) return '';
  return (
    <div class="pager">
      <a class="btn" href={href(props.basePath, { game: props.game, cursor: props.nextCursor })}>
        Older clips
      </a>
    </div>
  );
}

export function ClipGrid(props: { items: ClipWithOwner[]; empty: string }): JSX.Element {
  if (props.items.length === 0) return <p class="empty">{e(props.empty)}</p>;
  return <div class="grid">{props.items.map((item) => <ClipCard item={item} />)}</div>;
}
