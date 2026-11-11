// Importing clips from a folder, in two steps: Klipt checks the folder and shows what it will do
// (how many files need converting, how long it should take, how much space), the user confirms,
// then the Rust core imports in the background. This mirrors its `import-*` events, so the
// progress survives switching tabs.

import { api, errorMessage, on, type ImportPreview, type ImportProgress, type ImportSummary } from "./api";
import { summaryText } from "./importSummary";
import { toast } from "./toasts.svelte";

export type ImportPhase = "idle" | "checking" | "confirm" | "running";

export const importing = $state<{
  phase: ImportPhase;
  folder: string | null;
  /** While checking: files looked at so far. */
  checked: number;
  found: number;
  /** What the checked folder holds, for the confirmation. */
  preview: ImportPreview | null;
  progress: ImportProgress | null;
  /** The last finished import, kept while it has skipped files to show. */
  summary: ImportSummary | null;
}>({ phase: "idle", folder: null, checked: 0, found: 0, preview: null, progress: null, summary: null });

/** Step 1: look through `folder`; the confirmation opens when that's done. */
export async function checkFolder(folder: string): Promise<void> {
  if (importing.phase === "running" || importing.phase === "checking") {
    toast("An import is already running.", "info");
    return;
  }
  Object.assign(importing, { phase: "checking", folder, checked: 0, found: 0, preview: null });
  try {
    const preview = await api.checkImport(folder);
    // Stopped meanwhile (the cast: TypeScript keeps the narrowing from before the await).
    if ((importing.phase as ImportPhase) !== "checking") return;
    if (!preview) {
      importing.phase = "idle";
      return;
    }
    if (preview.inPlace + preview.remux + preview.reencode === 0) {
      importing.phase = "idle";
      toast(
        preview.files === 0
          ? "No videos or pictures in that folder."
          : preview.alreadyThere === preview.files
            ? "Everything in that folder is already in Klipt."
            : "Nothing in that folder can be imported.",
        "info",
      );
      return;
    }
    Object.assign(importing, { phase: "confirm", preview });
  } catch (e) {
    importing.phase = "idle";
    toast(errorMessage(e), "error");
  }
}

/** Step 2: the user said yes. */
export async function confirmImport(): Promise<void> {
  if (importing.phase !== "confirm" || !importing.folder) return;
  try {
    if (!(await api.importFolder(importing.folder))) {
      importing.phase = "idle";
      toast("Couldn't start the import. Check the folder again.", "error");
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
      toast(summaryText(summary), summary.imported > 0 ? "success" : "info");
    }),
  ];
  return () => stops.forEach((stop) => stop());
}

/** The whole import's progress, 0..1 (null before the first report). */
export function overallFraction(): number | null {
  return importing.phase === "running" && importing.progress ? importing.progress.fraction : null;
}
