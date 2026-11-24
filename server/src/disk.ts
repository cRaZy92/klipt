import { statfs } from 'node:fs/promises';

/** Free bytes for this (non-root) process on the filesystem holding `dir`, or null when it can't be read. */
export async function freeBytes(dir: string): Promise<number | null> {
  try {
    const s = await statfs(dir);
    return Number(s.bavail) * Number(s.bsize);
  } catch {
    return null;
  }
}

/**
 * Keeps the data disk from filling up. Postgres usually shares it, and a full disk can take the
 * database down with the uploads. New work (an upload, a compression) needs its own size plus
 * `minFreeBytes` free. Work that has already started (more bytes of an accepted upload) only stops
 * below a quarter of that, so an upload isn't cut off by its own bytes. When free space can't be
 * read, nothing is refused.
 */
export class DiskGuard {
  constructor(
    private dir: string,
    public minFreeBytes: number,
  ) {}

  free(): Promise<number | null> {
    return freeBytes(this.dir);
  }

  /** Whether `bytes` more can be written and still leave `minFreeBytes` free. */
  async roomFor(bytes: number): Promise<boolean> {
    const free = await this.free();
    return free === null || free - bytes >= this.minFreeBytes;
  }

  /** Whether free space is below the hard floor, where even started work must stop. */
  async belowFloor(): Promise<boolean> {
    const free = await this.free();
    return free !== null && free < this.minFreeBytes / 4;
  }

  /** Free space for the admin page; `low` once there's less than `minFreeBytes`. */
  async status(): Promise<{ freeBytes: number | null; low: boolean }> {
    const free = await this.free();
    return { freeBytes: free, low: free !== null && free < this.minFreeBytes };
  }
}
