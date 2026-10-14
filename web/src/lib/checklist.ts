// Checklists complete their task (D-80). Mirrors `streamline_domain::checklist::status_after`,
// so a tick shows the task done (or open again) before the server answers.

import type { ChecklistItem } from './api/types/ChecklistItem'

const allDone = (items: ChecklistItem[]) => items.length > 0 && items.every((i) => i.done)

/** The status a task moves to when its checklist changes from `old` to `next`, or null. */
export function statusAfter(old: ChecklistItem[], next: ChecklistItem[], status: string): 'done' | 'open' | null {
  if (status === 'open' && allDone(next) && !allDone(old)) return 'done'
  if (status === 'done' && next.some((n) => !n.done && old.some((o) => o.id === n.id && o.done))) return 'open'
  return null
}
