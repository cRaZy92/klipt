// Settings form helpers. `validateSettings` mirrors `Settings::validate` in `settings.rs`
// (same checks, order and messages); the core stays the authority and validates again on save.

import type { FieldError, Quality, Settings } from "./api";

export const BUFFER_SECONDS_MIN = 10;
export const BUFFER_SECONDS_MAX = 600;
export const BITRATE_KBPS_MIN = 2000;
export const BITRATE_KBPS_MAX = 80000;
export const FPS_CHOICES = [30, 60] as const;
export const CLEANUP_DAYS_MIN = 1;
export const CLEANUP_DAYS_MAX = 365;

/** The bitrate each quality preset stands for (`Quality::preset_kbps`). */
export const PRESET_KBPS: Record<Exclude<Quality, "custom">, number> = {
  low: 8000,
  medium: 14000,
  high: 20000,
};

/** The video bitrate the settings record with. */
export function bitrateKbps(s: Pick<Settings, "quality" | "customBitrateKbps">): number {
  return s.quality === "custom" ? s.customBitrateKbps : PRESET_KBPS[s.quality];
}

/** RAM the replay buffer needs: kbps × seconds / 8 → kB → MB, rounded. */
export function estimateRamMb(kbps: number, seconds: number): number {
  return Math.round((kbps * seconds) / 8 / 1000);
}

const inRange = (n: number, min: number, max: number) => Number.isInteger(n) && n >= min && n <= max;

export function validateSettings(s: Settings): FieldError[] {
  const errors: FieldError[] = [];
  const fail = (field: string, message: string) => errors.push({ field, message });
  if (s.bufferLocation === "disk") fail("bufferLocation", "Recording to disk isn't supported yet.");
  if (!(FPS_CHOICES as readonly number[]).includes(s.fps)) fail("fps", "Frame rate must be 30 or 60.");
  // Only Custom uses (and shows) the bitrate field; a stale value there must not block saving.
  if (s.quality === "custom" && !inRange(s.customBitrateKbps, BITRATE_KBPS_MIN, BITRATE_KBPS_MAX)) {
    fail("customBitrateKbps", "Bitrate must be between 2,000 and 80,000 kbps.");
  }
  if (!inRange(s.bufferSeconds, BUFFER_SECONDS_MIN, BUFFER_SECONDS_MAX)) {
    fail("bufferSeconds", "Clip length must be between 10 seconds and 10 minutes.");
  }
  if (!s.hotkey.trim()) fail("hotkey", "Choose a shortcut for saving clips.");
  if (!s.outputDir) fail("outputDir", "Choose a folder for your clips.");
  if (!inRange(s.cleanupDays, CLEANUP_DAYS_MIN, CLEANUP_DAYS_MAX)) fail("cleanupDays", "Keep clips for 1 to 365 days.");
  return errors;
}

/** True when both hold the same values (for the unsaved-changes guard). */
export function sameSettings(a: Settings, b: Settings): boolean {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)] as (keyof Settings)[]);
  return [...keys].every((k) => a[k] === b[k]);
}
