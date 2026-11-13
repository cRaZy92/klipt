// Which "Delete" choices make sense for the clips being deleted (one or several, from My clips).

export type RemoveWhere = "library" | "local" | "everywhere";

export interface RemovalTarget {
  /** Clips in the library on this PC: is the file still there, and is it uploaded (on the server)? */
  local: { fileMissing: boolean; uploaded: boolean }[];
  /** Uploads that aren't on this PC. */
  onlineOnly: number;
}

export interface RemovalChoices {
  library: boolean;
  local: boolean;
  everywhere: boolean;
  /** Picked when the dialog opens: the one that loses nothing that isn't kept elsewhere. */
  initial: RemoveWhere;
  /** Clips whose only copy "Delete from this PC" deletes. */
  notUploaded: number;
}

export function removalChoices({ local, onlineOnly }: RemovalTarget): RemovalChoices {
  const withFile = local.filter((c) => !c.fileMissing);
  const notUploaded = withFile.filter((c) => !c.uploaded).length;
  const choices = {
    library: local.length > 0,
    local: withFile.length > 0,
    everywhere: onlineOnly > 0 || local.some((c) => c.uploaded),
    notUploaded,
  };
  const initial: RemoveWhere = !choices.library
    ? "everywhere"
    : choices.local && notUploaded === 0
      ? "local"
      : "library";
  return { ...choices, initial };
}
