export type Toast = { id: number; text: string; kind: 'info' | 'error'; action?: { label: string; run: () => void } }

let next = 1
export const toasts = $state<Toast[]>([])

export function toast(text: string, kind: Toast['kind'] = 'info', action?: Toast['action']) {
  const t = { id: next++, text, kind, action }
  toasts.push(t)
  setTimeout(() => dismiss(t.id), action ? 6000 : 3500)
}

export function dismiss(id: number) {
  const i = toasts.findIndex((t) => t.id === id)
  if (i >= 0) toasts.splice(i, 1)
}
