// Pure helpers for the board and matrix views (SPEC §6.7). Views are presentation
// only: moving a card just changes the task field the view is grouped by.
import type { Task } from './api/types/Task'
import type { TaskPatch } from './store.svelte'

export type ViewKind = 'list' | 'board' | 'matrix'
export type BoardGroup = 'status' | 'project' | 'difficulty' | 'type'

/** Saved per scope in the user's preferences (`view:<scope>`). */
export type ViewPrefs = { view: ViewKind; group: BoardGroup }
export const DEFAULT_VIEW: ViewPrefs = { view: 'list', group: 'status' }

// ---- matrix -------------------------------------------------------------------

export type Quadrant = 'do' | 'schedule' | 'delegate' | 'eliminate'

/** Scores of 2 or more count as important/urgent (D-22). */
export const THRESHOLD = 2
const high = (v: number | null) => (v ?? 0) >= THRESHOLD

export const QUADRANTS: { id: Quadrant; title: string; hint: string; important: boolean; urgent: boolean }[] = [
  { id: 'do', title: 'Do first', hint: 'Urgent and important', important: true, urgent: true },
  { id: 'schedule', title: 'Schedule', hint: 'Important, not urgent', important: true, urgent: false },
  { id: 'delegate', title: 'Delegate or squeeze in', hint: 'Urgent, not important', important: false, urgent: true },
  { id: 'eliminate', title: 'Later or drop', hint: 'Neither', important: false, urgent: false },
]

export function quadrantOf(t: Pick<Task, 'importance' | 'urgency'>): Quadrant {
  const q = QUADRANTS.find((q) => q.important === high(t.importance) && q.urgent === high(t.urgency))
  return q!.id
}

/**
 * The change that moves a task into quadrant `q`. A score only changes when it is on
 * the wrong side of the threshold (to 2 when raised, cleared when lowered), so finer
 * values like 3 = "very important" survive moves along the other axis.
 */
export function matrixPatch(t: Pick<Task, 'importance' | 'urgency'>, q: Quadrant): TaskPatch {
  const target = QUADRANTS.find((x) => x.id === q)!
  const patch: TaskPatch = {}
  if (high(t.importance) !== target.important) patch.importance = target.important ? THRESHOLD : null
  if (high(t.urgency) !== target.urgent) patch.urgency = target.urgent ? THRESHOLD : null
  return patch
}

// ---- board --------------------------------------------------------------------

export type StatusColumn = 'todo' | 'doing' | 'waiting' | 'done'

export function statusColumn(t: Pick<Task, 'status' | 'started_at' | 'waiting_since'>): StatusColumn | null {
  if (t.status === 'done') return 'done'
  if (t.status !== 'open') return null
  if (t.waiting_since) return 'waiting'
  return t.started_at ? 'doing' : 'todo'
}

/** Difficulty column ids: '0' = not set, '1'..'3'. */
export const DIFFICULTY_COLUMNS = [
  { id: '0', title: 'No difficulty' },
  { id: '1', title: 'Easy' },
  { id: '2', title: 'Medium' },
  { id: '3', title: 'Hard' },
]
