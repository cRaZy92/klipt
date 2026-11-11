import { execFile } from 'node:child_process';

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
