// The editor timeline's zoom: which stretch of the clip the lanes show ("the view"), and the math
// to place things on it. At full view (the whole clip) everything sits where it did before zoom.

/** The stretch of the clip the timeline shows, in ms into the file. */
export interface TimeView {
  from: number;
  to: number;
}

/** The shortest stretch the timeline zooms in to. */
export const MIN_SPAN_MS = 1000;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** The whole clip. */
export function fullView(duration: number): TimeView {
  return { from: 0, to: Math.max(0, duration) };
}

/** How far the timeline can zoom in: the clip's length over MIN_SPAN_MS (1 = not at all). */
export function maxScale(duration: number): number {
  return duration > MIN_SPAN_MS ? duration / MIN_SPAN_MS : 1;
}

/** How far `view` is zoomed in: 1 = the whole clip, 2 = half of it, … */
export function viewScale(view: TimeView, duration: number): number {
  const span = view.to - view.from;
  return span > 0 && duration > 0 ? Math.max(1, duration / span) : 1;
}

/** Whether `view` shows less than the whole clip. */
export function isZoomed(view: TimeView, duration: number): boolean {
  return duration > 0 && (view.from > 0 || view.to < duration);
}

/**
 * `view` kept inside the clip, at least MIN_SPAN_MS (or the whole clip) long. The same object
 * when it already is.
 */
export function clampView(view: TimeView, duration: number): TimeView {
  if (!(duration > 0)) return view.from === 0 && view.to === 0 ? view : { from: 0, to: 0 };
  const span = clamp(view.to - view.from, Math.min(duration, MIN_SPAN_MS), duration);
  const from = clamp(Number.isFinite(view.from) ? view.from : 0, 0, duration - span);
  const to = from + span;
  return from === view.from && to === view.to ? view : { from, to };
}

/** `view` zoomed to `scale` (clamped to 1…maxScale), keeping `anchorMs` where it is on screen. */
export function zoomView(view: TimeView, duration: number, scale: number, anchorMs: number): TimeView {
  if (!(duration > 0)) return view;
  const span = duration / clamp(Number.isFinite(scale) ? scale : 1, 1, maxScale(duration));
  const oldSpan = view.to - view.from;
  const anchor = clamp(anchorMs, 0, duration);
  const share = oldSpan > 0 ? clamp((anchor - view.from) / oldSpan, 0, 1) : 0.5;
  const from = anchor - share * span;
  return clampView({ from, to: from + span }, duration);
}

/** `view` moved `deltaMs` later (earlier when negative), kept inside the clip. */
export function panView(view: TimeView, duration: number, deltaMs: number): TimeView {
  if (!Number.isFinite(deltaMs) || deltaMs === 0) return view;
  return clampView({ from: view.from + deltaMs, to: view.to + deltaMs }, duration);
}

/** `view` moved so it starts at `fromMs`, kept inside the clip. */
export function viewAt(view: TimeView, duration: number, fromMs: number): TimeView {
  return panView(view, duration, fromMs - view.from);
}

/**
 * `view` moved so `ms` (the playhead) shows: the same object when it already does. Run past the
 * right edge: the next page starts just before it. Back past the left edge: the page ends just
 * after it. A jump further than one page away (a click, the loop back to the start): centred.
 */
export function followView(view: TimeView, duration: number, ms: number): TimeView {
  if (!Number.isFinite(ms) || !(duration > 0) || (ms >= view.from && ms <= view.to)) return view;
  const span = view.to - view.from;
  let from: number;
  if (ms > view.to + span || ms < view.from - span) from = ms - span / 2;
  else if (ms > view.to) from = ms - span * 0.1;
  else from = ms - span * 0.9;
  return clampView({ from, to: from + span }, duration);
}

/** Where `ms` is on the timeline, in percent of its width (outside 0–100 when off screen). */
export function viewPct(ms: number, view: TimeView): number {
  const span = view.to - view.from;
  if (!(span > 0) || !Number.isFinite(ms)) return 0;
  return ((ms - view.from) / span) * 100;
}

/**
 * The time at `x` pixels on a timeline `width` pixels wide, in whole ms, inside the view (rounding
 * must not step past a view edge that isn't a whole ms: the view would turn the page).
 */
export function viewMs(x: number, width: number, view: TimeView): number {
  const span = view.to - view.from;
  if (!(span > 0) || !(width > 0) || !Number.isFinite(x)) return Math.round(view.from) || 0;
  const ms = Math.round(view.from + (clamp(x, 0, width) / width) * span);
  return Math.floor(view.to) > Math.ceil(view.from) ? clamp(ms, Math.ceil(view.from), Math.floor(view.to)) : ms;
}

/** Whether `ms` is on screen. */
export function inView(ms: number, view: TimeView): boolean {
  return ms >= view.from && ms <= view.to;
}

/** The part of `fromMs`–`toMs` that is on screen, in percent of the width; null when none is. */
export function viewSpan(fromMs: number, toMs: number, view: TimeView): { left: number; width: number } | null {
  const left = Math.max(0, viewPct(fromMs, view));
  const right = Math.min(100, viewPct(toMs, view));
  return right > left ? { left, width: right - left } : null;
}

/** The zoom slider's position (0–1, logarithmic) for `scale`. */
export function scaleToSlider(scale: number, duration: number): number {
  const max = maxScale(duration);
  if (max <= 1) return 0;
  return clamp(Math.log(Math.max(1, scale)) / Math.log(max), 0, 1);
}

/** The scale at the zoom slider's position `value` (0–1). */
export function sliderToScale(value: number, duration: number): number {
  return Math.pow(maxScale(duration), clamp(Number.isFinite(value) ? value : 0, 0, 1));
}

/**
 * The part of the clip to read waveform peaks for when the whole track's `wholeBuckets` would
 * give fewer than `perView` across `view` (long recordings, zoomed far in); null when they're
 * enough. The part is snapped to steps of the view's length (rounded up to a power of 2) and
 * covers a step either side, so panning reuses it until the view crosses a step.
 */
export function peakWindow(view: TimeView, duration: number, wholeBuckets: number, perView = 2048): TimeView | null {
  const span = view.to - view.from;
  if (!(duration > 0) || !(span > 0) || (span / duration) * wholeBuckets >= perView) return null;
  const step = 2 ** Math.ceil(Math.log2(span));
  const from = Math.max(0, Math.floor(view.from / step) * step - step);
  return { from, to: Math.min(duration, from + 3 * step) };
}

/** The scrollbar's thumb: where the view is in the whole clip, in percent. */
export function scrollThumb(view: TimeView, duration: number): { left: number; width: number } {
  if (!(duration > 0)) return { left: 0, width: 100 };
  return { left: (view.from / duration) * 100, width: ((view.to - view.from) / duration) * 100 };
}
