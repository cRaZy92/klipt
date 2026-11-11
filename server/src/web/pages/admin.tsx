import type { AdminStats, AdminTranscodes, AdminUser, Invite, TranscodeJob } from '@klipt/shared';
import type { AuthUser } from '../../auth/sessions.js';
import { formatDuration, formatGB, formatSize, relativeDate, skDate } from '../components.js';
import { Layout } from '../layout.js';

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
      <div class="stats">
        <div class="stat"><b>{formatSize(props.data.savedBytes)}</b><span>saved</span></div>
        <div class="stat"><b>{counts.queued + counts.running}</b><span>in queue</span></div>
        <div class="stat"><b>{counts.done}</b><span>compressed</span></div>
        <div class="stat"><b>{counts.failed}</b><span>failed</span></div>
      </div>
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

export function AdminPage(props: {
  user: AuthUser;
  stats: AdminStats;
  invites: Invite[];
  users: AdminUser[];
  transcodes: AdminTranscodes;
}): JSX.Element {
  const { stats } = props;
  return (
    <Layout title="Admin" user={props.user}>
      <h1>Admin</h1>
      <div class="stats">
        <div class="stat"><b>{stats.users}</b><span>users</span></div>
        <div class="stat"><b>{stats.clips}</b><span>clips</span></div>
        <div class="stat"><b>{formatGB(stats.storageBytes)}</b><span>storage</span></div>
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

      <h2>Users</h2>
      <ul class="rows plain">
        {props.users.map((u) => (
          <li class="row" data-user-id={u.id}>
            <div class="row-main">
              <a href={`/u/${encodeURIComponent(u.username)}`}><strong>{e(u.username)}</strong></a>
              {u.isAdmin ? <span class="badge accent">admin</span> : ''}
              {u.disabled ? <span class="badge">disabled</span> : ''}
              <div class="meta"><span>{u.clipCount} clips · {formatSize(u.storageBytes)}</span><span>joined {relativeDate(new Date(u.createdAt))}</span></div>
            </div>
            {u.id === props.user.id ? (
              ''
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
