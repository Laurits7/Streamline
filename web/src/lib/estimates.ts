// Better estimates (10.5): learn from finished tasks how long things really take compared
// with their estimate (focus time spent), per project, per type of task and overall, and
// suggest realistic estimates. A median keeps one runaway task from skewing it.

import type { Task } from './api/types/Task'

export type Factor = { factor: number; n: number }
export type Model = {
  overall: Factor | null
  byProject: Map<string | null, Factor>
  byType: Map<string, Factor>
}
export type Basis = 'project' | 'type' | 'overall'

/** Samples needed before a group (or the overall factor) is trusted. */
export const MIN_GROUP = 3
export const MIN_OVERALL = 5
/** Only the most recent finished tasks count: habits change. */
const RECENT = 200

const median = (xs: number[]) => {
  const s = [...xs].sort((a, b) => a - b)
  const m = Math.floor(s.length / 2)
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
}
const clamp = (x: number) => Math.min(4, Math.max(0.25, x))

/** Finished tasks with an estimate and focus time logged: actual ÷ estimate. */
export function samples(tasks: Iterable<Task>): { task: Task; ratio: number }[] {
  return [...tasks]
    .filter((t) => t.status === 'done' && !t.deleted_at && (t.estimate_min ?? 0) >= 5 && t.actual_min > 0)
    .sort((a, b) => (b.completed_at ?? '').localeCompare(a.completed_at ?? ''))
    .slice(0, RECENT)
    .map((t) => ({ task: t, ratio: t.actual_min / t.estimate_min! }))
}

function group<K>(xs: { task: Task; ratio: number }[], key: (t: Task) => K, min: number): Map<K, Factor> {
  const by = new Map<K, number[]>()
  for (const s of xs) by.set(key(s.task), [...(by.get(key(s.task)) ?? []), s.ratio])
  const out = new Map<K, Factor>()
  for (const [k, rs] of by) if (rs.length >= min) out.set(k, { factor: clamp(median(rs)), n: rs.length })
  return out
}

export function learn(tasks: Iterable<Task>): Model {
  const xs = samples(tasks)
  return {
    overall: xs.length >= MIN_OVERALL ? { factor: clamp(median(xs.map((s) => s.ratio))), n: xs.length } : null,
    byProject: group(xs, (t) => t.project_id, MIN_GROUP),
    byType: group(xs, (t) => t.task_type_id, MIN_GROUP),
  }
}

/** The most specific factor known for a task: its project, its type, else overall. */
export function factorFor(model: Model, task: Pick<Task, 'project_id' | 'task_type_id'>): (Factor & { basis: Basis }) | null {
  const p = model.byProject.get(task.project_id)
  if (p) return { ...p, basis: 'project' }
  const t = model.byType.get(task.task_type_id)
  if (t) return { ...t, basis: 'type' }
  return model.overall ? { ...model.overall, basis: 'overall' } : null
}

/** An estimate scaled by a factor, rounded to 5 minutes (at least 5). */
export const realistic = (estimate: number, factor: number) => Math.max(5, Math.round((estimate * factor) / 5) * 5)

/** A suggestion for a task's estimate, when experience says it's clearly off (> 15 %). */
export function suggestion(model: Model, task: Task): (Factor & { basis: Basis; minutes: number }) | null {
  if (!task.estimate_min) return null
  const f = factorFor(model, task)
  if (!f || Math.abs(f.factor - 1) < 0.15) return null
  const minutes = realistic(task.estimate_min, f.factor)
  return minutes === task.estimate_min ? null : { ...f, minutes }
}

/** For Trends: share of finished tasks within ±25 % of their estimate, and per-week medians. */
export function accuracy(tasks: Iterable<Task>, weekOf: (iso: string) => string) {
  const xs = samples(tasks)
  const within = xs.filter((s) => s.ratio >= 0.75 && s.ratio <= 1.25).length
  const byWeek = new Map<string, number[]>()
  for (const s of xs) {
    const w = weekOf(s.task.completed_at!)
    byWeek.set(w, [...(byWeek.get(w) ?? []), s.ratio])
  }
  return {
    n: xs.length,
    within: xs.length ? within / xs.length : null,
    weeks: [...byWeek.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .slice(-8)
      .map(([week, rs]) => ({ week, factor: median(rs), n: rs.length })),
  }
}
