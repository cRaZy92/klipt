// Waveform peaks: per bucket, the loudest absolute sample over all channels.

/** `buckets` values in 0–1; empty when there are no buckets or no samples. */
export function peaks(channels: Float32Array[], buckets: number): Float32Array {
  const length = channels.reduce((max, c) => Math.max(max, c.length), 0);
  if (buckets <= 0 || length === 0) return new Float32Array(0);
  const out = new Float32Array(buckets);
  for (const channel of channels) {
    for (let b = 0; b < buckets; b++) {
      const end = Math.min(channel.length, Math.floor(((b + 1) * length) / buckets));
      let max = out[b];
      for (let i = Math.floor((b * length) / buckets); i < end; i++) {
        const v = Math.abs(channel[i]);
        if (v > max) max = v;
      }
      out[b] = max;
    }
  }
  return out;
}

/**
 * `fine` peaks pooled down to `buckets` (the loudest of each group). Exactly what `peaks` would
 * give over the same samples when `fine.length` is a multiple of `buckets`; null otherwise.
 */
export function poolPeaks(fine: Float32Array, buckets: number): Float32Array | null {
  if (buckets <= 0 || fine.length < buckets || fine.length % buckets !== 0) return null;
  if (fine.length === buckets) return fine;
  const k = fine.length / buckets;
  const out = new Float32Array(buckets);
  for (let b = 0; b < buckets; b++) {
    let max = 0;
    for (let i = b * k; i < (b + 1) * k; i++) if (fine[i] > max) max = fine[i];
    out[b] = max;
  }
  return out;
}

/**
 * One value per pixel for a lane `width` pixels wide showing the share `from`–`to` (0–1) of the
 * clip: the loudest bucket under each pixel, or the bucket a pixel falls in where there are fewer
 * buckets than pixels.
 */
export function windowPeaks(p: Float32Array, from: number, to: number, width: number): Float32Array {
  const out = new Float32Array(Math.max(0, Math.floor(width)));
  const n = p.length;
  if (!n || !out.length || !(to > from)) return out;
  const first = from * n;
  const per = ((to - from) * n) / out.length;
  for (let x = 0; x < out.length; x++) {
    const start = Math.min(n - 1, Math.max(0, Math.floor(first + x * per)));
    const end = Math.min(n, Math.max(start + 1, Math.floor(first + (x + 1) * per)));
    let max = 0;
    for (let i = start; i < end; i++) if (p[i] > max) max = p[i];
    out[x] = max;
  }
  return out;
}
