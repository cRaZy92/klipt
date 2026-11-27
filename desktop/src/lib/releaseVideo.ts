/**
 * Lets go of a media element's player right away. WebKitGTK keeps a removed <video>'s GStreamer
 * pipeline until garbage collection, and tearing several down at once stalls the page for a
 * moment (hover previews froze My clips this way). Emptying `src` and calling load() frees it now.
 */
export function releaseVideo(video: HTMLMediaElement | undefined | null) {
  if (!video) return;
  video.pause();
  video.removeAttribute("src");
  video.load();
}
