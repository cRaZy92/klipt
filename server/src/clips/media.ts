import { execFile } from 'node:child_process';

export type MediaErrorCode = 'not_a_video' | 'unsupported_codec';

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

function run(bin: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(bin, args, { maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) reject(new Error(`${bin} failed: ${stderr || err.message}`));
      else resolve(stdout);
    });
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
  let parsed: { streams?: ProbeStream[]; format?: { duration?: string } };
  try {
    const out = await run(ffprobeBin(), [
      '-v', 'error',
      '-print_format', 'json',
      '-show_format',
      '-show_streams',
      path,
    ]);
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

export async function makeThumbnail(
  input: string,
  output: string,
  durationMs: number,
): Promise<void> {
  const atMs = Math.min(1000, durationMs / 2);
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
