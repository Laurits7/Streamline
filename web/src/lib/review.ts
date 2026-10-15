// The weekly review (10.4): what last week looked like, what's been left lying around, and
// which projects have nothing to do next. Pure, so it's tested; the view acts on it.

import type { DayEntry } from './api/types/DayEntry'
import type { FocusSession } from './api/types/FocusSession'
import type { Project } from './api/types/Project'
import type { Task } from './api/types/Task'

export type WeekStats = {
  /** Tasks completed during the week. */
  done: number
  /** Tasks planned on the week's days, and how many of those got done. */
  planned: number
  plannedDone: number
  /** Planned tasks that were missed (expired at day end). */
  missed: number
  focusMinutes: number
  /** Projects with the most completed tasks, most first (null = inbox). */
  topProjects: { projectId: string | null; done: number }[]
}

/** Dates are local `YYYY-MM-DD`; `[from, to)` is the week. Completion times are UTC ISO. */
export function weekStats(
  tasks: Task[],
  entries: DayEntry[],
  sessions: FocusSession[],
  from: string,
  to: string,
  dateOf: (iso: string) => string,
): WeekStats {
  const inWeek = (d: string) => d >= from && d < to
  const done = tasks.filter((t) => t.status === 'done' && t.completed_at && inWeek(dateOf(t.completed_at)))
  const byId = new Map(tasks.map((t) => [t.id, t]))
  const plannedTasks = entries.filter((e) => !e.deleted_at && inWeek(e.date)).map((e) => byId.get(e.task_id)).filter((t): t is Task => !!t)
  const counts = new Map<string | null, number>()
  for (const t of done) counts.set(t.project_id, (counts.get(t.project_id) ?? 0) + 1)
  return {
    done: done.length,
    planned: plannedTasks.length,
    plannedDone: plannedTasks.filter((t) => t.status === 'done').length,
    missed: plannedTasks.filter((t) => t.status === 'missed').length,
    focusMinutes: sessions.filter((s) => s.kind === 'work' && !s.deleted_at && inWeek(dateOf(s.started_at))).reduce((n, s) => n + s.minutes, 0),
    topProjects: [...counts.entries()]
      .map(([projectId, n]) => ({ projectId, done: n }))
      .sort((a, b) => b.done - a.done)
      .slice(0, 3),
  }
}

export type LooseEnd = { task: Task; why: 'overdue' | 'carried' | 'stale' }

/** Untouched for this long (and not planned) counts as stale. */
export const STALE_DAYS = 21

/**
 * Open tasks that need a decision: overdue, carried over three times or more, or untouched
 * for three weeks and not planned. Each task once, with its most pressing reason.
 */
export function looseEnds(tasks: Task[], planned: (taskId: string) => boolean, today: string, nowMs: number): LooseEnd[] {
  const staleBefore = new Date(nowMs - STALE_DAYS * 864e5).toISOString()
  const out: LooseEnd[] = []
  for (const t of tasks) {
    if (t.status !== 'open' || t.deleted_at || t.waiting_since || t.series_id) continue
    if (t.due_date && t.due_date < today) out.push({ task: t, why: 'overdue' })
    else if (t.carry_count >= 3) out.push({ task: t, why: 'carried' })
    else if (t.updated_at < staleBefore && !planned(t.id)) out.push({ task: t, why: 'stale' })
  }
  const order = { overdue: 0, carried: 1, stale: 2 }
  return out.sort((a, b) => order[a.why] - order[b.why] || a.task.created_at.localeCompare(b.task.created_at))
}

/** Active projects where nothing is left to do, counting their subprojects. */
export function projectsWithoutNextStep(projects: Project[], tasks: Task[], subtree: (id: string) => string[]): Project[] {
  const open = new Set(tasks.filter((t) => t.status === 'open' && !t.deleted_at && t.project_id).map((t) => t.project_id))
  return projects
    .filter((p) => !p.deleted_at && !p.archived_at && p.status === 'active' && !p.parent_id)
    .filter((p) => !subtree(p.id).some((id) => open.has(id)))
}
