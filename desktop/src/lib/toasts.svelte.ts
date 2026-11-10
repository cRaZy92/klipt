// Short messages in the corner: "Link copied!", recorder problems, and so on.

export type ToastKind = "info" | "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

let nextId = 1;

export const toasts = $state<Toast[]>([]);

export function dismissToast(id: number) {
  const index = toasts.findIndex((t) => t.id === id);
  if (index >= 0) toasts.splice(index, 1);
}

/** Shows a toast; errors stay longer. The same message isn't stacked twice. */
export function toast(message: string, kind: ToastKind = "info") {
  const existing = toasts.find((t) => t.message === message);
  if (existing) dismissToast(existing.id);
  const id = nextId++;
  toasts.push({ id, kind, message });
  if (toasts.length > 4) toasts.shift();
  setTimeout(() => dismissToast(id), kind === "error" ? 8000 : 3500);
}
