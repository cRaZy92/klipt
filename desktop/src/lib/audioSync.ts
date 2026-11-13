// The editor plays each audio track in its own <audio> next to a muted <video>; this keeps the
// tracks on the video's clock. Pure logic (the elements and the clock are passed in), tested in
// audioSync.test.ts.
//
// A track moved while playing takes a moment to start again (WebKitGTK: 30 ms to well over
// 250 ms, depending on the machine's load), and says it's playing all the while. Moving it again
// before it started kept it silent for good, so a moved track is left alone until it plays, and
// how far it fell behind meanwhile is learned and added to the next move.

/** What the sync needs of an <audio> or <video> element. */
export interface Media {
  currentTime: number;
  readonly paused: boolean;
  readonly seeking: boolean;
}

/** A track more than this far off the video is moved back in step. */
export const MAX_DRIFT_S = 0.12;
/** A moved track that hasn't started after this long is moved again. */
const SETTLE_TIMEOUT_MS = 2000;
/** The most a track is moved ahead of the video to make up for its restart. */
const MAX_LEAD_S = 0.5;

export class AudioSync {
  /** How far behind a track lands after a move while playing: added to the next move. */
  private lead = 0;
  /** Tracks just moved and not playing again yet: left alone until they are. */
  private settling = new Map<Media, { target: number; at: number }>();

  /** `now` is a millisecond clock (performance.now in the app). */
  constructor(private now: () => number) {}

  /** Puts every track where the video is (after a seek, or when playback starts). */
  align(video: Media, tracks: Media[]) {
    for (const track of tracks) this.move(video, track);
  }

  /** Runs every 250 ms while the editor is open: moves tracks that drifted. */
  check(video: Media, tracks: Media[]) {
    if (video.paused || video.seeking) return;
    for (const track of tracks) {
      if (track.seeking || track.paused) continue;
      const moved = this.settling.get(track);
      if (moved) {
        const started = track.currentTime > moved.target + 0.01;
        if (!started && this.now() - moved.at < SETTLE_TIMEOUT_MS) continue;
        this.settling.delete(track);
        // Whatever it's still behind is how long it takes to start again: lead by that next time.
        if (started) {
          this.lead = Math.min(MAX_LEAD_S, Math.max(0, this.lead + video.currentTime - track.currentTime));
        }
      }
      if (Math.abs(track.currentTime - video.currentTime) > MAX_DRIFT_S) this.move(video, track);
    }
  }

  private move(video: Media, track: Media) {
    // A paused video waits for the tracks, so there's nothing to make up for.
    const target = video.currentTime + (video.paused ? 0 : this.lead);
    track.currentTime = target;
    this.settling.set(track, { target, at: this.now() });
  }
}
