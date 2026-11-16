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
