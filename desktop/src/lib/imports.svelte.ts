// Importing clips from a folder, in two steps: Klipt checks the folder and shows what it will do
// (how many files need converting, how long it should take, how much space), the user confirms,
// then the Rust core imports in the background. This mirrors its `import-*` events, so the
// progress survives switching tabs. Files and folders dropped on the window go the same way;
// a quick drop (nothing to convert) starts without asking, since picking the drop zone said yes.

import { api, errorMessage, on, type ImportPreview, type ImportProgress, type ImportSummary } from "./api";
import { summaryText } from "./importSummary";
import { toast } from "./toasts.svelte";

export type ImportPhase = "idle" | "checking" | "confirm" | "running";

/** What is being imported: a picked folder, or what was dropped (`copy`: into the clips folder). */
export type ImportSource = { kind: "folder"; folder: string } | { kind: "drop"; paths: string[]; copy: boolean };

/** A drop that would take longer than this asks first. */
const ASK_AFTER_SECS = 60;

export const importing = $state<{
  phase: ImportPhase;
  source: ImportSource | null;
  /** While checking: files looked at so far. */
  checked: number;
  found: number;
  /** What the checked folder holds, for the confirmation. */
  preview: ImportPreview | null;
  progress: ImportProgress | null;
  /** The last finished import, kept while it has skipped files to show. */
  summary: ImportSummary | null;
  /** A quick drop is starting without asking: the confirmation stays hidden. */
  autoStart: boolean;
}>({ phase: "idle", source: null, checked: 0, found: 0, preview: null, progress: null, summary: null, autoStart: false });

/** Files in a preview that will be imported. */
export function toImportCount(p: ImportPreview): number {
  return p.inPlace + p.copied + p.remux + p.reencode;
}

/** Step 1: look through `folder`; the confirmation opens when that's done. */
export function checkFolder(folder: string): Promise<void> {
  return check({ kind: "folder", folder });
}

/** Files and folders dropped on the window: kept where they are, or copied into the clips folder. */
export function checkDropped(paths: string[], copy: boolean): Promise<void> {
  return check({ kind: "drop", paths, copy });
}

async function check(source: ImportSource): Promise<void> {
  if (importing.phase !== "idle") {
    toast(importing.phase === "confirm" ? "Finish or cancel the import that's waiting first." : "An import is already running.", "info");
    return;
  }
  Object.assign(importing, { phase: "checking", source, checked: 0, found: 0, preview: null });
  try {
    const preview = source.kind === "folder" ? await api.checkImport(source.folder) : await api.checkDrop(source.paths, source.copy);
    // Stopped meanwhile (the cast: TypeScript keeps the narrowing from before the await).
    if ((importing.phase as ImportPhase) !== "checking") return;
    if (!preview) {
      importing.phase = "idle";
      return;
    }
    if (toImportCount(preview) === 0) {
      importing.phase = "idle";
      const where = source.kind === "folder" ? "in that folder" : "you dropped";
      toast(
        preview.files === 0
          ? `No videos or pictures ${where}.`
          : preview.alreadyThere === preview.files
            ? `Everything ${where} is already in Klipt.`
            : `Nothing ${where} can be imported.`,
        "info",
      );
      return;
    }
    const quick = source.kind === "drop" && preview.reencode === 0 && preview.estimateSecs <= ASK_AFTER_SECS;
    Object.assign(importing, { phase: "confirm", preview, autoStart: quick });
    if (quick) {
      await confirmImport();
      importing.autoStart = false;
    }
  } catch (e) {
    importing.phase = "idle";
    toast(errorMessage(e), "error");
  }
}

/** Step 2: the user said yes. */
export async function confirmImport(): Promise<void> {
  const source = importing.source;
  if (importing.phase !== "confirm" || !source) return;
  try {
    const started = source.kind === "folder" ? await api.importFolder(source.folder) : await api.importDrop();
    if (!started) {
      importing.phase = "idle";
      toast(`Couldn't start the import. ${source.kind === "folder" ? "Check the folder" : "Drop the files"} again.`, "error");
      return;
    }
    Object.assign(importing, { phase: "running", progress: null, summary: null });
  } catch (e) {
    importing.phase = "idle";
    toast(errorMessage(e), "error");
  }
}

/** Closes the confirmation, or stops checking. */
export function closeImport(): void {
  if (importing.phase === "checking") api.cancelImport().catch(() => {});
  if (importing.phase !== "running") Object.assign(importing, { phase: "idle", preview: null });
}

/** Stops a running import (clips already imported stay). */
export function stopImport(): void {
  api.cancelImport().catch(() => {});
}

export function dismissImportSummary(): void {
  importing.summary = null;
}

/** Follows the core's import events for the whole app; returns a function that stops. */
export function watchImports(): () => void {
  const stops = [
    on("import-check-progress", ({ checked, total }) => {
      if (importing.phase === "checking") Object.assign(importing, { checked, found: total });
    }),
    on("import-progress", (progress) => {
      importing.progress = progress;
      if (importing.phase !== "running") importing.phase = "running";
    }),
    on("import-finished", (summary) => {
      Object.assign(importing, {
        phase: "idle",
        progress: null,
        preview: null,
        summary: summary.skipped.length > 0 ? summary : null,
      });
      const copied = importing.source?.kind === "drop" && importing.source.copy;
      toast(summaryText(summary, copied), summary.imported > 0 ? "success" : "info");
    }),
  ];
  return () => stops.forEach((stop) => stop());
}

/** The whole import's progress, 0..1 (null before the first report). */
export function overallFraction(): number | null {
  return importing.phase === "running" && importing.progress ? importing.progress.fraction : null;
}
