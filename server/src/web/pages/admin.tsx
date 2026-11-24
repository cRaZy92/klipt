import type { AdminStats, AdminTranscodes, AdminUser, FeedbackItem, Invite, TranscodeJob } from '@klipt/shared';
import type { GameShare, WeekActivity } from '../../admin/queries.js';
import type { AuthUser } from '../../auth/sessions.js';
import { formatDuration, formatGB, formatSize, relativeDate, skDate } from '../components.js';
import { Avatar, Layout } from '../layout.js';

import { esc as e } from '../escape.js';

const STATE_LABEL: Record<TranscodeJob['state'], string> = {
  queued: 'Queued',
  running: 'Compressing',
  done: 'Done',
  skipped: 'Kept original',
  failed: 'Failed',
};

/** "41 s", "3 min 5 s". */
function took(job: TranscodeJob): string {
  if (!job.startedAt || !job.finishedAt) return '';
  const s = Math.max(0, Math.round((Date.parse(job.finishedAt) - Date.parse(job.startedAt)) / 1000));
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`;
}

function JobRow(props: { job: TranscodeJob }): JSX.Element {
  const j = props.job;
  const pct = j.progress !== null ? Math.round(j.progress * 100) : null;
  let sizes = formatSize(j.sizeBytes);
  if (j.state === 'done' && j.originalBytes) {
    const saved = Math.round((1 - j.sizeBytes / j.originalBytes) * 100);
    sizes = `${formatSize(j.originalBytes)} → ${formatSize(j.sizeBytes)} (−${saved}%)`;
  }
  return (
    <li class="row job" data-clip-id={j.clipId}>
      <div class="row-main">
        <a href={`/c/${j.clipId}`}><strong>{e(j.title)}</strong></a>
        <span class={`badge job-${j.state}`}>{STATE_LABEL[j.state]}</span>
        <div class="meta">
          <span>{e(j.owner)}</span>
          <span>{formatDuration(j.durationMs)}</span>
          <span>{sizes}</span>
          {j.state === 'running' && j.startedAt ? <span>started {relativeDate(new Date(j.startedAt))}</span> : ''}
          {took(j) ? <span>took {took(j)}</span> : ''}
          {j.finishedAt ? <span>{relativeDate(new Date(j.finishedAt))}</span> : ''}
        </div>
        {pct !== null ? (
          <div class="job-progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={String(pct)}>
            <div style={`width:${pct}%`}></div>
            <span>{pct}%</span>
          </div>
        ) : (
          ''
        )}
        {j.error ? (
          <details class="job-error">
            <summary>What went wrong</summary>
            <pre>{e(j.error)}</pre>
          </details>
        ) : (
          ''
        )}
      </div>
      {j.state === 'failed' ? (
        <button type="button" class="btn small" data-action="retry-transcode">Retry</button>
      ) : (
        ''
      )}
    </li>
  );
}

/** The compression queue. app.js reloads it from /admin/transcodes while there's work. */
export function TranscodeSection(props: { data: AdminTranscodes }): JSX.Element {
  const { counts, active, recent } = props.data;
  const busy = counts.queued + counts.running > 0;
  const hidden = counts.queued + counts.running - active.length;
  return (
    <section id="transcodes" data-busy={String(busy)}>
      <p class="muted section-note">
        Uploaded clips are re-encoded to H.264 in the background, one at a time, so they take less space and still play on every
        device. A clip plays from its upload until that's done.
      </p>
      {!props.data.enabled ? <p class="empty">Compression is turned off on this server (TRANSCODE=off).</p> : ''}
      <QueueBar counts={counts} saved={props.data.savedBytes} />
      {active.length > 0 ? (
        <>
          <h3>Now</h3>
          <ul class="rows plain">{active.map((j) => <JobRow job={j} />)}</ul>
          {hidden > 0 ? <p class="muted">…and {hidden} more waiting.</p> : ''}
        </>
      ) : (
        <p class="empty">Nothing waiting.</p>
      )}
      {recent.length > 0 ? (
        <>
          <h3>Recently finished</h3>
          <ul class="rows plain">{recent.map((j) => <JobRow job={j} />)}</ul>
        </>
      ) : (
        ''
      )}
    </section>
  );
}

/** Rounds a chart's top up to 1, 2 or 5 times a power of ten, so the gridlines get even labels. */
function niceMax(n: number): number {
  if (n <= 4) return 4;
  const p = 10 ** Math.floor(Math.log10(n));
  return ([1, 2, 5, 10].find((m) => m * p >= n) ?? 10) * p;
}

/** "10. 11." */
function dayMonth(isoDay: string): string {
  const [, m, d] = isoDay.split('-');
  return `${Number(d)}. ${Number(m)}.`;
}

/** Uploads per week: a column per week, the newest on the right, with a tooltip on each. */
function ActivityChart(props: { weeks: WeekActivity[] }): JSX.Element {
  const { weeks } = props;
  const top = niceMax(Math.max(...weeks.map((w) => w.clips)));
  const total = weeks.reduce((n, w) => n + w.clips, 0);
  const clipsLabel = (n: number) => (n === 1 ? '1 clip' : `${n} clips`);
  return (
    <figure class="panel chart-panel">
      <figcaption>
        <h3>Uploads per week</h3>
        <p class="muted">{clipsLabel(total)} in the last 12 weeks</p>
      </figcaption>
      <div class="cols">
        <div class="cols-grid" aria-hidden="true">
          <span style="--v:1">{String(top)}</span>
          <span style="--v:.5">{String(top / 2)}</span>
          <span style="--v:0">0</span>
        </div>
        {weeks.map((w, i) => {
          const last = i === weeks.length - 1;
          const label = `${last ? 'This week' : `Week of ${dayMonth(w.week)}`}: ${clipsLabel(w.clips)}${w.clips ? `, ${formatSize(w.bytes)}` : ''}`;
          return (
            <div class={`col${last ? ' now' : ''}${i < 2 ? ' start' : i > weeks.length - 3 ? ' end' : ''}`} style={`--v:${w.clips / top}`} tabindex="0" aria-label={e(label)}>
              <div class="col-bar">{last && w.clips > 0 ? <span class="col-value">{String(w.clips)}</span> : ''}</div>
              <span class="tip" role="presentation">
                <b>{last ? 'This week' : `Week of ${dayMonth(w.week)}`}</b>
                {clipsLabel(w.clips)}
                {w.clips ? <span>{formatSize(w.bytes)}</span> : ''}
              </span>
              <span class="col-x" aria-hidden="true">{last ? 'Now' : i % 3 === 0 ? dayMonth(w.week) : ''}</span>
            </div>
          );
        })}
      </div>
    </figure>
  );
}

/** Clips per game: horizontal bars, biggest first, the rest folded into "Other games". */
function GamesChart(props: { games: GameShare[] }): JSX.Element {
  const max = Math.max(1, ...props.games.map((g) => g.clips));
  return (
    <figure class="panel chart-panel">
      <figcaption>
        <h3>Top games</h3>
        <p class="muted">By number of clips</p>
      </figcaption>
      {props.games.length === 0 ? (
        <p class="muted">No clips with a game yet.</p>
      ) : (
        <ul class="hbars">
          {props.games.map((g) => (
            <li class={g.gameName ? '' : 'other'} title={e(`${g.gameName ?? 'Other games'}: ${g.clips}`)}>
              <span class="hbar-name">{e(g.gameName ?? 'Other games')}</span>
              <span class="hbar-track"><span class="hbar" style={`--v:${g.clips / max}`}></span></span>
              <span class="hbar-value">{String(g.clips)}</span>
            </li>
          ))}
        </ul>
      )}
    </figure>
  );
}

/** Where every uploaded video stands in the compression queue, as one bar split by state. */
function QueueBar(props: { counts: AdminTranscodes['counts']; saved: number }): JSX.Element {
  const { counts } = props;
  const parts: [string, string, number][] = [
    ['done', 'Compressed', counts.done],
    ['skipped', 'Kept original', counts.skipped],
    ['waiting', 'Waiting', counts.queued + counts.running],
    ['failed', 'Failed', counts.failed],
  ];
  const total = parts.reduce((n, [, , v]) => n + v, 0);
  return (
    <div class="panel queue">
      <div class="queue-head">
        <div>
          <b class="big">{formatSize(props.saved)}</b>
          <span class="muted">saved by compressing</span>
        </div>
        <span class="muted">{total === 1 ? '1 video' : `${total} videos`}</span>
      </div>
      {total > 0 ? (
        <div class="seg" role="img" aria-label={e(parts.map(([, l, v]) => `${l} ${v}`).join(', '))}>
          {parts.filter(([, , v]) => v > 0).map(([k, l, v]) => <span class={`seg-${k}`} style={`flex-grow:${v}`} title={e(`${l}: ${v}`)}></span>)}
        </div>
      ) : (
        ''
      )}
      <ul class="legend">
        {parts.map(([k, l, v]) => (
          <li><i class={`seg-${k}`}></i>{l} <b>{String(v)}</b></li>
        ))}
      </ul>
    </div>
  );
}

export function AdminPage(props: {
  user: AuthUser;
  stats: AdminStats;
  invites: Invite[];
  users: AdminUser[];
  transcodes: AdminTranscodes;
  activity: WeekActivity[];
  games: GameShare[];
  feedback: FeedbackItem[];
}): JSX.Element {
  const { stats } = props;
  const openReports = props.feedback.filter((f) => !f.resolvedAt).length;
  const thisWeek = props.activity.at(-1)?.clips ?? 0;
  const users = [...props.users].sort((a, b) => b.storageBytes - a.storageBytes);
  const maxBytes = Math.max(1, ...users.map((u) => u.storageBytes));
  return (
    <Layout title="Admin" user={props.user} nav="admin">
      <h1>Admin</h1>
      <div class="stats">
        <div class="stat"><b>{stats.users}</b><span>users</span></div>
        <div class="stat"><b>{stats.clips}</b><span>clips</span></div>
        <div class="stat"><b>{formatGB(stats.storageBytes)}</b><span>storage</span></div>
        {stats.freeBytes !== null && (
          <div
            class={stats.diskLow ? 'stat low' : 'stat'}
            title={stats.diskLow ? 'Low on disk space: new uploads and compression wait until there is more room' : 'Free space on the server\'s data disk'}
          >
            <b>{formatGB(stats.freeBytes)}</b>
            <span>{stats.diskLow ? 'free, uploads wait' : 'free'}</span>
          </div>
        )}
        <div class="stat"><b>{thisWeek}</b><span>{thisWeek === 1 ? 'clip this week' : 'clips this week'}</span></div>
      </div>

      <div class="charts">
        <ActivityChart weeks={props.activity} />
        <GamesChart games={props.games} />
      </div>

      <h2>Video compression</h2>
      <TranscodeSection data={props.transcodes} />

      <h2>Invites</h2>
      <button type="button" class="btn primary" data-action="create-invite">Create invite</button>
      {props.invites.length === 0 ? <p class="empty">No open invites. Create one and send the link to a friend.</p> : ''}
      <ul class="rows plain" id="invites">
        {props.invites.map((i) => (
          <li class="row" data-code={e(i.code)}>
            <div class="row-main">
              <code class="url">{e(i.url)}</code>
              <div class="meta"><span>Expires {skDate(i.expiresAt)}</span></div>
            </div>
            <button type="button" class="btn small" data-action="copy" data-url={e(i.url)}>Copy</button>
            <button type="button" class="btn small danger" data-action="revoke-invite">Revoke</button>
          </li>
        ))}
      </ul>

      <h2 id="feedback">
        Reports and ideas {openReports > 0 ? <span class="badge accent">{openReports} open</span> : ''}
      </h2>
      <FeedbackSection items={props.feedback} />

      <h2>Users</h2>
      <ul class="rows plain">
        {users.map((u) => (
          <li class="row user-row" data-user-id={u.id}>
            <Avatar name={u.displayName} size="md" />
            <div class="row-main">
              <a href={`/u/${encodeURIComponent(u.username)}`}><strong>{e(u.displayName)}</strong></a>
              {u.displayName !== u.username ? <span class="muted"> @{e(u.username)}</span> : ''}
              {u.isAdmin ? <span class="badge accent">admin</span> : ''}
              {u.disabled ? <span class="badge">disabled</span> : ''}
              <div class="meta">
                <span>{u.clipCount === 1 ? '1 clip' : `${u.clipCount} clips`}</span>
                <span>joined {relativeDate(new Date(u.createdAt))}</span>
                <span>{u.lastSeenAt ? `seen ${relativeDate(new Date(u.lastSeenAt))}` : 'never seen'}</span>
              </div>
              <div class="meta device" data-role="device">
                {u.app ? (
                  <>
                    <span>{u.app.version ? `Klipt ${e(u.app.version)}` : 'Klipt app'}</span>
                    {u.app.os ? <span>{e(u.app.os)}</span> : ''}
                    {u.app.gpu ? <span>{e(u.app.gpu)}</span> : ''}
                  </>
                ) : (
                  <span>No app signed in</span>
                )}
              </div>
            </div>
            <div class="usage" title="Storage on the server">
              <span class="usage-track"><span class="usage-bar" style={`--v:${u.storageBytes / maxBytes}`}></span></span>
              <span class="usage-value">{formatSize(u.storageBytes)}</span>
            </div>
            <button type="button" class="btn small" data-action="reset-password" title="Make a one-time link to set a new password">
              Password link
            </button>
            {u.id === props.user.id ? (
              <span class="btn-slot"></span>
            ) : (
              <button type="button" class="btn small" data-action="toggle-user" data-disabled={String(u.disabled)}>
                {u.disabled ? 'Enable' : 'Disable'}
              </button>
            )}
          </li>
        ))}
      </ul>
    </Layout>
  );
}

/** Problem reports and ideas from friends: open ones first, each with what it was sent with. */
function FeedbackSection(props: { items: FeedbackItem[] }): JSX.Element {
  if (props.items.length === 0) return <p class="empty">Nothing yet. Friends can send problems and ideas from the app's Settings or their Account page.</p>;
  return (
    <ul class="rows plain feedback-list">
      {props.items.map((f) => (
        <li class={`row feedback-row${f.resolvedAt ? ' resolved' : ''}`} data-feedback-id={f.id}>
          <div class="row-main">
            <div>
              <span class={`badge ${f.kind === 'bug' ? 'job-failed' : 'accent'}`}>{f.kind === 'bug' ? 'Problem' : 'Idea'}</span>
              <strong> {e(f.user.displayName)}</strong>
              <span class="meta">
                <span>{relativeDate(new Date(f.createdAt))}</span>
                <span>{f.source === 'desktop' ? 'from the app' : 'from the website'}</span>
                {f.resolvedAt ? <span>resolved</span> : ''}
              </span>
            </div>
            <p class="feedback-message">{e(f.message)}</p>
            {f.system ? (
              <details>
                <summary>System info</summary>
                <dl class="sysinfo">
                  {Object.entries(f.system).map(([k, v]) => (
                    <>
                      <dt>{e(k)}</dt>
                      <dd>{e(v)}</dd>
                    </>
                  ))}
                </dl>
              </details>
            ) : (
              ''
            )}
          </div>
          {f.hasLogs ? (
            <a class="btn small" href={`/api/admin/feedback/${e(f.id)}/logs`} target="_blank" rel="noopener">
              Open log
            </a>
          ) : (
            ''
          )}
          <button type="button" class="btn small" data-action="resolve-feedback" data-resolved={String(!!f.resolvedAt)}>
            {f.resolvedAt ? 'Reopen' : 'Resolve'}
          </button>
          <button type="button" class="btn small danger" data-action="delete-feedback">Delete</button>
        </li>
      ))}
    </ul>
  );
}
