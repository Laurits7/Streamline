// Goals review timing. Mirror of crates/domain/src/goals.rs `period_start` / `review_due`.
import type { Goal } from './api/types/Goal'

export type Cadence = 'off' | 'weekly' | 'monthly'

const addDays = (date: string, n: number) => {
  const [y, m, d] = date.split('-').map(Number)
  return new Date(Date.UTC(y, m - 1, d + n)).toISOString().slice(0, 10)
}

/** First day of the current review period (null when reviews are off). */
export function periodStart(cadence: Cadence, today: string, weekStart: number): string | null {
  if (cadence === 'off') return null
  if (cadence === 'monthly') return today.slice(0, 8) + '01'
  const iso = ((new Date(today + 'T00:00:00Z').getUTCDay() + 6) % 7) + 1
  const ws = Math.min(7, Math.max(1, weekStart))
  return addDays(today, -((iso + 7 - ws) % 7))
}

export function reviewDue(cadence: Cadence, lastReview: string | null, today: string, weekStart: number): boolean {
  const start = periodStart(cadence, today, weekStart)
  return start !== null && (lastReview === null || lastReview < start)
}

// ---- the goal tree (D-112) -------------------------------------------------------

type TreeGoal = Pick<Goal, 'id' | 'parent_id' | 'position' | 'kind'>

/** Goals in tree order (each followed by its sub-goals, siblings by position) with their
 *  depth. A goal whose parent isn't among `goals` (closed, deleted) starts a tree. */
export function goalTree<G extends TreeGoal>(goals: G[]): { goal: G; depth: number; children: number }[] {
  const ids = new Set(goals.map((g) => g.id))
  const kids = new Map<string | null, G[]>()
  for (const g of goals) {
    const p = g.parent_id && ids.has(g.parent_id) ? g.parent_id : null
    kids.set(p, [...(kids.get(p) ?? []), g])
  }
  for (const list of kids.values()) list.sort((a, b) => (a.position < b.position ? -1 : a.position > b.position ? 1 : a.id.localeCompare(b.id)))
  const out: { goal: G; depth: number; children: number }[] = []
  const seen = new Set<string>()
  const walk = (parent: string | null, depth: number) => {
    for (const g of kids.get(parent) ?? []) {
      if (seen.has(g.id)) continue
      seen.add(g.id)
      out.push({ goal: g, depth, children: kids.get(g.id)?.length ?? 0 })
      walk(g.id, depth + 1)
    }
  }
  walk(null, 0)
  return out
}

/** `id` and every goal under it. */
export function subtreeIds(goals: TreeGoal[], id: string): Set<string> {
  const out = new Set([id])
  for (let grew = true; grew; ) {
    grew = false
    for (const g of goals)
      if (g.parent_id && out.has(g.parent_id) && !out.has(g.id)) {
        out.add(g.id)
        grew = true
      }
  }
  return out
}

/** Goals `id` can be moved under: not rules, not itself or one of its sub-goals. */
export function parentOptions<G extends TreeGoal>(goals: G[], id: string | null): G[] {
  const exclude = id ? subtreeIds(goals, id) : new Set<string>()
  return goals.filter((g) => g.kind !== 'rule' && !exclude.has(g.id))
}

/** Which of a goal's lists holds its links to tasks, projects or routines. */
export const LINK_FIELD = { task: 'task_ids', project: 'project_ids', series: 'series_ids' } as const
export type LinkKind = keyof typeof LINK_FIELD
