/** Where a clip's media lives in the clips folder. */

/** A clip's video: the upload is `<id>.mp4`; each replacement (compression) gets a new name, `<id>.v<n>.mp4`. */
export function videoFileName(id: string, version: number): string {
  return version > 0 ? `${id}.v${version}.mp4` : `${id}.mp4`;
}

/** The file `/media/` serves as a clip's video (or a screenshot's picture). */
export function mediaFileName(clip: { id: string; kind: 'clip' | 'screenshot'; mediaVersion: number }): string {
  return clip.kind === 'screenshot' ? `${clip.id}.png` : videoFileName(clip.id, clip.mediaVersion);
}

/** A name `/media/` may serve: `<id>.jpg|png|mp4` or `<id>.v<n>.mp4`. Group 1 is the clip id. */
export const MEDIA_FILE = /^([A-Za-z0-9_-]{12})(?:\.(?:jpg|png|mp4)|\.v[1-9]\d{0,5}\.mp4)$/;
