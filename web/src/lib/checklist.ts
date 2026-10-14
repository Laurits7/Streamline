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

type Rect = { top: number; left: number; width: number; height: number }
const GAP = 6
const MARGIN = 8

/**
 * Where to put a checklist popover (top-left corner) next to a timeline item (D-81): to its
 * right if it fits, otherwise to its left, otherwise below or above it; always kept inside
 * the viewport so items near an edge still show the whole list.
 */
export function placePopover(
  anchor: Rect,
  pop: { width: number; height: number },
  view: { width: number; height: number },
): { top: number; left: number } {
  const clamp = (v: number, max: number) => Math.round(Math.max(MARGIN, Math.min(Math.max(MARGIN, max), v)))
  const maxLeft = view.width - pop.width - MARGIN
  const maxTop = view.height - pop.height - MARGIN
  const right = anchor.left + anchor.width + GAP
  if (right + pop.width + MARGIN <= view.width) return { top: clamp(anchor.top, maxTop), left: Math.round(right) }
  const left = anchor.left - GAP - pop.width
  if (left >= MARGIN) return { top: clamp(anchor.top, maxTop), left: Math.round(left) }
  const x = clamp(anchor.left, maxLeft)
  const below = anchor.top + anchor.height + GAP
  if (below + pop.height + MARGIN <= view.height) return { top: Math.round(below), left: x }
  const above = anchor.top - GAP - pop.height
  if (above >= MARGIN) return { top: Math.round(above), left: x }
  return { top: clamp(maxTop, maxTop), left: x }
}
