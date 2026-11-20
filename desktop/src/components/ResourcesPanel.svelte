<script lang="ts">
  // Settings → Resources: what Klipt's processes use, sampled once a second while this is open.
  import { api, errorMessage, type ResourceUsage } from "../lib/api";
  import { formatSize } from "../lib/format";
  import {
    HISTORY,
    PART_INFO,
    formatPercent,
    pushSample,
    scaleTop,
    sparkArea,
    sparkPath,
    totals,
    type Metric,
    type Totals,
  } from "../lib/resources";

  let usage = $state<ResourceUsage | null>(null);
  let history = $state<Totals[]>([]);
  let error = $state<string | null>(null);

  $effect(() => {
    let busy = false;
    let stopped = false;
    async function poll() {
      // A hidden window (Klipt in the tray) needs no numbers.
      if (busy || document.hidden) return;
      busy = true;
      try {
        const next = await api.resourceUsage();
        if (stopped) return;
        usage = next;
        history = pushSample(history, totals(next));
        error = null;
      } catch (e) {
        if (!stopped) error = errorMessage(e);
      } finally {
        busy = false;
      }
    }
    poll();
    const timer = setInterval(poll, 1000);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  });

  const MB = 1024 ** 2;
  const W = 240;
  const H = 48;

  const tiles = $derived.by(() => {
    if (!usage) return [];
    const u = usage;
    const tile = (metric: Metric, label: string, format: (v: number) => string, floor: number, of: string | null) => {
      const values = history.map((h) => h[metric]);
      const top = scaleTop(values, floor);
      return {
        metric,
        label,
        format,
        values,
        now: values.at(-1) ?? null,
        of,
        line: sparkPath(values, top, W, H),
        area: sparkArea(values, top, W, H),
      };
    };
    return [
      tile("ram", "RAM", formatSize, 256 * MB, `of ${formatSize(u.totalRamBytes)}`),
      tile("cpu", "CPU", formatPercent, 5, `of ${u.cpuThreads} threads`),
      tile("gpu", "GPU", formatPercent, 5, u.gpuName),
      tile("vram", "VRAM", formatSize, 256 * MB, u.totalVramBytes ? `of ${formatSize(u.totalVramBytes)}` : null),
    ];
  });

  // The sample under the pointer, per graph: index into `history`.
  let hover = $state<{ metric: Metric; index: number } | null>(null);

  function onMove(e: PointerEvent, metric: Metric) {
    const box = (e.currentTarget as SVGElement).getBoundingClientRect();
    const slot = Math.round(((e.clientX - box.left) / box.width) * (HISTORY - 1));
    const index = slot - (HISTORY - history.length);
    hover = index >= 0 && index < history.length ? { metric, index } : null;
  }

  function hoverX(index: number): number {
    return ((HISTORY - history.length + index) / (HISTORY - 1)) * W;
  }

  function ago(index: number): string {
    const s = history.length - 1 - index;
    return s === 0 ? "now" : `${s} s ago`;
  }
</script>

<section>
  <h2>Resources</h2>
  <p class="intro muted">What Klipt uses right now. Measured every second, only while this page is open.</p>

  {#if error && !usage}
    <p class="form-error" role="alert">{error}</p>
  {:else if !usage}
    <p class="muted">Measuring…</p>
  {:else}
    <div class="tiles">
      {#each tiles as t (t.metric)}
        {@const missing = t.now == null}
        {@const h = hover?.metric === t.metric ? hover.index : null}
        <div class="tile" class:missing>
          <span class="label">{t.label}</span>
          <strong class="value">{missing ? "—" : t.format(h != null ? (t.values[h] ?? 0) : t.now!)}</strong>
          <small class="muted">
            {#if h != null}{ago(h)}{:else if missing}Not available{:else}<span title={t.of}>{t.of ?? " "}</span>{/if}
          </small>
          <svg
            class="spark"
            viewBox="0 0 {W} {H}"
            preserveAspectRatio="none"
            role="img"
            aria-label="{t.label} over the last minute"
            onpointermove={(e) => onMove(e, t.metric)}
            onpointerleave={() => (hover = null)}
          >
            <line class="base" x1="0" y1={H - 0.5} x2={W} y2={H - 0.5} />
            <path class="area" d={t.area} />
            <path class="line" d={t.line} />
            {#if h != null && t.values[h] != null}
              <line class="cross" x1={hoverX(h)} y1="0" x2={hoverX(h)} y2={H} />
            {/if}
          </svg>
        </div>
      {/each}
    </div>
    {#if usage.gpuNote}<p class="note muted">{usage.gpuNote}</p>{/if}

    <table>
      <thead>
        <tr>
          <th scope="col">Part</th>
          <th scope="col">RAM</th>
          <th scope="col">CPU</th>
          <th scope="col">GPU</th>
          <th scope="col">VRAM</th>
        </tr>
      </thead>
      <tbody>
        {#each usage.parts as p (p.part)}
          {@const idle = p.processes === 0}
          <tr class:idle>
            <th scope="row">
              <strong>{PART_INFO[p.part].label}</strong><br />
              <small class="muted">{idle ? "Not running" : PART_INFO[p.part].detail}</small>
            </th>
            <td>{idle ? "—" : formatSize(p.ramBytes)}</td>
            <td>{idle ? "—" : formatPercent(p.cpuPercent)}</td>
            <td>{idle || p.gpuPercent == null ? "—" : formatPercent(p.gpuPercent)}</td>
            <td>{idle || p.vramBytes == null ? "—" : formatSize(p.vramBytes)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="note muted">
      CPU is a share of the whole processor. GPU is the busiest of drawing, encoding and decoding. Memory shared
      with other programs, like system libraries, counts as Klipt's fair share.
    </p>
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
    padding: 1.4rem 1.5rem 1.5rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: linear-gradient(180deg, var(--surface), rgba(21, 20, 29, 0.55));
  }

  h2 {
    font-size: 1.2rem;
    font-weight: 700;
  }

  .intro {
    margin-top: -0.65rem;
    font-size: 0.9rem;
  }

  .muted {
    color: var(--text-muted);
  }

  section {
    container-type: inline-size;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.75rem;
  }

  @container (max-width: 560px) {
    .tiles {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .tile {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
    padding: 0.85rem 0.9rem 0.7rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
  }

  .label {
    color: var(--text-muted);
    font-size: 0.8rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .value {
    font-family: var(--font-display);
    font-size: 1.6rem;
    font-variant-numeric: tabular-nums;
    line-height: 1.15;
  }

  .tile.missing .value {
    color: var(--text-faint);
  }

  .tile small {
    overflow: hidden;
    font-size: 0.8rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .spark {
    width: 100%;
    height: 48px;
    margin-top: 0.45rem;
    overflow: visible;
    cursor: crosshair;
  }

  .spark .base {
    stroke: var(--border-strong);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }

  .spark .area {
    fill: var(--accent-soft);
  }

  .spark .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
    vector-effect: non-scaling-stroke;
  }

  .spark .cross {
    stroke: var(--text-muted);
    stroke-width: 1;
    stroke-dasharray: 3 3;
    vector-effect: non-scaling-stroke;
  }

  .note {
    margin-top: -0.4rem;
    font-size: 0.82rem;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }

  th,
  td {
    padding: 0.6rem 0.5rem;
    border-bottom: 1px solid var(--border);
    text-align: right;
    white-space: nowrap;
  }

  thead th {
    color: var(--text-muted);
    font-size: 0.8rem;
    font-weight: 600;
  }

  th:first-child {
    padding-left: 0;
    text-align: left;
    white-space: normal;
  }

  td:last-child,
  th:last-child {
    padding-right: 0;
  }

  tbody th {
    font-weight: 400;
    line-height: 1.35;
  }

  tbody th small {
    font-size: 0.8rem;
  }

  tbody tr:last-child > * {
    border-bottom: none;
  }

  tr.idle td,
  tr.idle strong {
    color: var(--text-faint);
  }
</style>
