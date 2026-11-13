// What My clips remembers while the app runs, so leaving it (the editor, Settings, Feed) and coming
// back shows the same filters, the same clips at once, and the same scroll position.

import type { ClipView, MyUpload, Sort } from "./api";
import type { MediaKind, Place } from "./myClips";

export interface LibraryMemory {
  place: Place;
  game: string | null;
  sort: Sort;
  search: string;
  media: MediaKind;
  /** The app's scroll area position (px). */
  scrollTop: number;
  /** The lists last shown; null until first loaded. They're refreshed on return. */
  local: ClipView[] | null;
  uploads: MyUpload[] | null;
}

function fresh(): LibraryMemory {
  return {
    place: "all",
    game: null,
    sort: "newestFirst",
    search: "",
    media: "all",
    scrollTop: 0,
    local: null,
    uploads: null,
  };
}

export const libraryMemory: LibraryMemory = fresh();

/** Forgets everything, e.g. on logout (the next account mustn't see these uploads). */
export function forgetLibrary() {
  Object.assign(libraryMemory, fresh());
}
