// Inbox triage (10.3): go through what's waiting in the inbox one item at a time and decide
// with one key. What's in the queue, which key does what, and where the session stands
// (with undo) are pure here; the view applies the decisions.

import type { Task } from './api/types/Task'

export type Decision = 'done' | 'today' | 'tomorrow' | 'day' | 'project' | 'waiting' | 'drop' | 'skip' | 'focus'

/** The keys, in the order the buttons show. */
export const DECISIONS: { decision: Decision; key: string; label: string; hint: string }[] = [
  { decision: 'done', key: 'x', label: 'Done', hint: 'Already done, or took two minutes' },
  { decision: 'focus', key: 'f', label: 'Do now', hint: 'Start focusing on it' },
  { decision: 'today', key: 't', label: 'Today', hint: 'Plan it for today' },
  { decision: 'tomorrow', key: 'm', label: 'Tomorrow', hint: 'Plan it for tomorrow' },
  { decision: 'day', key: 'd', label: 'Pick a day', hint: 'Plan it for another day' },
  { decision: 'project', key: 'p', label: 'To a project', hint: 'File it under a project' },
  { decision: 'waiting', key: 'a', label: 'Waiting', hint: 'Waiting for someone or something' },
  { decision: 'drop', key: 'Delete', label: "Won't do", hint: 'Drop it (you can undo)' },
  { decision: 'skip', key: 's', label: 'Skip', hint: 'Leave it in the inbox for now' },
]

/** What a key press means during triage (also arrows and Backspace), or null. */
export function decisionFor(key: string): Decision | 'undo' | null {
  if (key === 'u' || key === 'z') return 'undo'
  if (key === 'Backspace') return 'drop'
  if (key === 'ArrowRight' || key === ' ') return 'skip'
  return DECISIONS.find((d) => d.key === key)?.decision ?? null
}

/**
 * What still needs a decision: open tasks without a project that aren't planned, waiting,
 * or a routine's occurrence. Oldest first, so nothing sits at the bottom forever.
 */
export function triageQueue(tasks: Iterable<Task>, planned: (taskId: string) => boolean): Task[] {
  return [...tasks]
    .filter((t) => t.status === 'open' && !t.project_id && !t.deleted_at && !t.waiting_since && !t.series_id && !planned(t.id))
    .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id))
}

export type Step = { id: string; decision: Decision }

/** A triage session over a fixed list of ids (taken when it starts). */
export type Session = { ids: string[]; at: number; history: Step[] }

export const start = (ids: string[]): Session => ({ ids, at: 0, history: [] })

/** The item being decided, or null when every one has been seen. */
export const current = (s: Session): string | null => s.ids[s.at] ?? null

/** Record a decision on the current item and move on. */
export function decide(s: Session, decision: Decision): Session {
  const id = current(s)
  if (!id) return s
  return { ...s, at: s.at + 1, history: [...s.history, { id, decision }] }
}

/** Step back to the previous item; returns the step to reverse, if any. */
export function undo(s: Session): { session: Session; step: Step | null } {
  const step = s.history.at(-1) ?? null
  if (!step) return { session: s, step: null }
  return { session: { ...s, at: s.at - 1, history: s.history.slice(0, -1) }, step }
}

/** How the session went, for the end screen. */
export function tally(s: Session): Partial<Record<Decision, number>> {
  const out: Partial<Record<Decision, number>> = {}
  for (const h of s.history) out[h.decision] = (out[h.decision] ?? 0) + 1
  return out
}
