// Short messages in the corner: "Link copied!", recorder problems, and so on.

export type ToastKind = "info" | "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  /** A button on the toast, e.g. "Copy link". */
  action?: ToastAction;
}

export interface ToastAction {
  label: string;
  run: () => void;
}

let nextId = 1;

export const toasts = $state<Toast[]>([]);

export function dismissToast(id: number) {
  const index = toasts.findIndex((t) => t.id === id);
  if (index >= 0) toasts.splice(index, 1);
}

/** Shows a toast; errors and ones with a button stay longer. The same message isn't stacked twice. */
export function toast(message: string, kind: ToastKind = "info", action?: ToastAction) {
  const existing = toasts.find((t) => t.message === message);
  if (existing) dismissToast(existing.id);
  const id = nextId++;
  toasts.push(action ? { id, kind, message, action } : { id, kind, message });
  if (toasts.length > 4) toasts.shift();
  setTimeout(() => dismissToast(id), kind === "error" || action ? 8000 : 3500);
}
