import { execFile, spawn, type ChildProcess } from 'node:child_process';
import { setPriority } from 'node:os';

export type MediaErrorCode = 'not_a_video' | 'unsupported_codec' | 'corrupt_video';

export class MediaError extends Error {
  constructor(
    public code: MediaErrorCode,
    message?: string,
  ) {
    super(message ?? code);
    this.name = 'MediaError';
  }
}

export interface VideoInfo {
  durationMs: number;
  width: number;
  height: number;
  videoCodec: string;
  audioCodec: string | null;
}

const ffmpegBin = () => process.env.FFMPEG_PATH || 'ffmpeg';
const ffprobeBin = () => process.env.FFPROBE_PATH || 'ffprobe';

const RUN_TIMEOUT_MS = 10 * 60 * 1000;

/**
 * ffmpeg errors that are the server's fault, not the video's: a full or read-only disk, missing
 * permissions, no memory. These must not get an upload deleted as a corrupt video.
 */
const SERVER_FAULT =
  /No space left on device|Disk quota exceeded|Read-only file system|Permission denied|Cannot allocate memory|Too many open files/i;

/**
 * Runs ffmpeg/ffprobe. A non-zero exit usually means the tool rejected the input, so it becomes a
 * MediaError('corrupt_video'), unless stderr shows a server-side fault (SERVER_FAULT). Anything
 * else (spawn failure such as ENOENT/ENOSPC, a timeout kill, an output overflow) is an
 * operational problem and stays a plain Error.
 */
function run(bin: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(
      bin,
      args,
      { maxBuffer: 16 * 1024 * 1024, timeout: RUN_TIMEOUT_MS },
      (err, stdout, stderr) => {
        if (!err) return resolve(stdout);
        const detail = `${bin} failed: ${stderr || err.message}`;
        const rejected = typeof err.code === 'number' && err.code !== 0 && !SERVER_FAULT.test(stderr);
        if (rejected) reject(new MediaError('corrupt_video', detail));
        else reject(new Error(detail, { cause: err }));
      },
    );
  });
}

interface ProbeStream {
  codec_type?: string;
  codec_name?: string;
  width?: number;
  height?: number;
  duration?: string;
}

export async function probeVideo(path: string): Promise<VideoInfo> {
  let out: string;
  try {
    out = await run(ffprobeBin(), [
      '-v', 'error',
      '-print_format', 'json',
      '-show_format',
      '-show_streams',
      path,
    ]);
  } catch (err) {
    // ffprobe refusing the file means it isn't a video; spawn/timeout errors propagate.
    if (err instanceof MediaError) throw new MediaError('not_a_video', err.message);
    throw err;
  }
  let parsed: { streams?: ProbeStream[]; format?: { duration?: string } };
  try {
    parsed = JSON.parse(out);
  } catch {
    throw new MediaError('not_a_video');
  }
  const streams = parsed.streams ?? [];
  const video = streams.find((s) => s.codec_type === 'video');
  if (!video || !video.width || !video.height) throw new MediaError('not_a_video');
  const audio = streams.find((s) => s.codec_type === 'audio');
  const seconds = Number(parsed.format?.duration ?? video.duration);
  if (!Number.isFinite(seconds)) throw new MediaError('not_a_video');
  return {
    durationMs: Math.round(seconds * 1000),
    width: video.width,
    height: video.height,
    videoCodec: video.codec_name ?? 'unknown',
    audioCodec: audio ? (audio.codec_name ?? 'unknown') : null,
  };
}

export function assertPlayable(info: VideoInfo): void {
  if (info.videoCodec !== 'h264' || (info.audioCodec !== null && info.audioCodec !== 'aac')) {
    throw new MediaError(
      'unsupported_codec',
      `unsupported codec: video=${info.videoCodec} audio=${info.audioCodec ?? 'none'}`,
    );
  }
}

export async function remuxFaststart(input: string, output: string): Promise<void> {
  await run(ffmpegBin(), [
    '-v', 'error', '-y',
    '-i', input,
    '-c', 'copy',
    '-movflags', '+faststart',
    '-f', 'mp4',
    output,
  ]);
}

/** Where the thumbnail is taken: the uploader's frame kept inside the video, else 1 s in (or the middle of a shorter clip). */
export function thumbnailAtMs(durationMs: number, requestedMs?: number | null): number {
  if (requestedMs == null) return Math.min(1000, durationMs / 2);
  return Math.min(Math.max(0, requestedMs), Math.max(0, durationMs - 100));
}

export async function makeThumbnail(
  input: string,
  output: string,
  durationMs: number,
  requestedMs?: number | null,
): Promise<void> {
  const atMs = thumbnailAtMs(durationMs, requestedMs);
  await run(ffmpegBin(), [
    '-v', 'error', '-y',
    '-ss', (atMs / 1000).toFixed(3),
    '-i', input,
    '-frames:v', '1',
    '-vf', 'scale=640:-2',
    '-f', 'image2',
    '-c:v', 'mjpeg',
    output,
  ]);
}

export interface ImageInfo {
  width: number;
  height: number;
}

/** A still picture (PNG, JPEG, WebP…) rather than a video: its size, or null for anything else. */
export async function probeImage(path: string): Promise<ImageInfo | null> {
  let out: string;
  try {
    out = await run(ffprobeBin(), ['-v', 'error', '-print_format', 'json', '-show_format', '-show_streams', path]);
  } catch (err) {
    if (err instanceof MediaError) return null;
    throw err;
  }
  let parsed: { streams?: ProbeStream[]; format?: { format_name?: string } };
  try {
    parsed = JSON.parse(out);
  } catch {
    return null;
  }
  const format = parsed.format?.format_name ?? '';
  // ffmpeg's still-image demuxers are `image2` and `<codec>_pipe` (png_pipe, jpeg_pipe, webp_pipe…).
  if (format !== 'image2' && !format.endsWith('_pipe')) return null;
  const streams = parsed.streams ?? [];
  const video = streams.find((s) => s.codec_type === 'video');
  if (!video?.width || !video.height || streams.length !== 1) return null;
  return { width: video.width, height: video.height };
}

/** Writes `input`'s picture as a PNG. */
export async function toPng(input: string, output: string): Promise<void> {
  await run(ffmpegBin(), ['-v', 'error', '-y', '-i', input, '-frames:v', '1', '-f', 'image2', '-c:v', 'png', output]);
}

export interface TranscodeOptions {
  /** x264 quality: lower is better and bigger (23 is x264's default). */
  crf: number;
  /** x264 speed/size trade-off, e.g. `medium`. */
  preset: string;
  /** CPU threads for decoding and encoding, so compression leaves the rest of the server's cores alone. */
  threads: number;
  /** The input's length, for progress. */
  durationMs: number;
  /** Called with 0..1 as ffmpeg works through the clip. */
  onProgress?: (fraction: number) => void;
  /** Hands over the ffmpeg process, so a caller can stop it. */
  onSpawn?: (child: ChildProcess) => void;
}

/**
 * Re-encodes a clip's video to H.264 (yuv420p, plays everywhere, including older iPhones) at a
 * constant quality, copying the audio and keeping faststart. ffmpeg runs at a lower CPU priority so
 * the website stays responsive. Rejects like `run`: a MediaError when ffmpeg refuses the input,
 * a plain Error for anything else (including being killed).
 */
export function transcodeH264(input: string, output: string, opts: TranscodeOptions): Promise<void> {
  const args = [
    '-v', 'error', '-y', '-nostdin',
    '-threads', String(opts.threads),
    '-i', input,
    '-map', '0:v:0', '-map', '0:a?',
    '-c:v', 'libx264', '-preset', opts.preset, '-crf', String(opts.crf), '-threads', String(opts.threads),
    '-filter_threads', '1',
    '-pix_fmt', 'yuv420p', '-profile:v', 'high',
    '-c:a', 'copy',
    '-movflags', '+faststart',
    '-progress', 'pipe:1', '-nostats',
    '-f', 'mp4',
    output,
  ];
  // Generous: a slow server may encode a long 1440p clip at a fraction of real time.
  const timeoutMs = Math.max(RUN_TIMEOUT_MS, opts.durationMs * 20);
  return new Promise((resolve, reject) => {
    const child = spawn(ffmpegBin(), args, { stdio: ['ignore', 'pipe', 'pipe'] });
    try {
      if (child.pid) setPriority(child.pid, 10);
    } catch {
      // Not allowed to lower it here: run at normal priority.
    }
    opts.onSpawn?.(child);
    let stderr = '';
    let pending = '';
    child.stderr.on('data', (d: Buffer) => {
      stderr = (stderr + d.toString()).slice(-4000);
    });
    child.stdout.on('data', (d: Buffer) => {
      pending += d.toString();
      const lines = pending.split('\n');
      pending = lines.pop() ?? '';
      for (const line of lines) {
        const m = /^out_time_us=(\d+)/.exec(line);
        if (m && opts.durationMs > 0) opts.onProgress?.(Math.min(1, Number(m[1]) / 1000 / opts.durationMs));
      }
    });
    const timer = setTimeout(() => child.kill('SIGKILL'), timeoutMs);
    child.on('error', (err) => {
      clearTimeout(timer);
      reject(new Error(`ffmpeg failed: ${err.message}`, { cause: err }));
    });
    child.on('close', (code, signal) => {
      clearTimeout(timer);
      if (code === 0) return resolve();
      const detail = `ffmpeg failed: ${stderr.trim() || (signal ? `killed (${signal})` : `exit ${code}`)}`;
      if (code !== null && !SERVER_FAULT.test(stderr)) reject(new MediaError('corrupt_video', detail));
      else reject(new Error(detail));
    });
  });
}
