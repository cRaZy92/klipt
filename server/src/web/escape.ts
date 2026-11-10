const MAP: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

/** Escapes a user-controlled string for HTML text or attribute context. */
export function esc(value: string): string {
  return value.replace(/[&<>"']/g, (c) => MAP[c]!);
}
