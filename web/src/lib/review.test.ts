import { describe, expect, it } from 'vitest'
import type { DayEntry } from './api/types/DayEntry'
import type { FocusSession } from './api/types/FocusSession'
import type { Project } from './api/types/Project'
import type { Task } from './api/types/Task'
import { looseEnds, projectsWithoutNextStep, weekStats } from './review'

const T0 = '2026-10-15T12:00:00Z'
const task = (id: string, extra: Partial<Task> = {}) =>
  ({ id, status: 'open', project_id: null, deleted_at: null, waiting_since: null, series_id: null, due_date: null, carry_count: 0,
    completed_at: null, created_at: '2026-10-01T00:00:00Z', updated_at: T0, ...extra }) as Task
const entry = (task_id: string, date: string) => ({ id: `e${task_id}`, task_id, date, deleted_at: null }) as DayEntry
const dateOf = (iso: string) => iso.slice(0, 10)

describe('weekly review (10.4)', () => {
  it('sums up the week', () => {
    const tasks = [
      task('a', { status: 'done', completed_at: '2026-10-06T09:00:00Z', project_id: 'P' }),
      task('b', { status: 'done', completed_at: '2026-10-07T09:00:00Z', project_id: 'P' }),
      task('c', { status: 'done', completed_at: '2026-10-08T09:00:00Z' }),
      task('d', { status: 'done', completed_at: '2026-10-13T09:00:00Z' }), // next week
      task('e', { status: 'missed' }),
      task('f'),
    ]
    const entries = [entry('a', '2026-10-06'), entry('e', '2026-10-09'), entry('f', '2026-10-10'), entry('d', '2026-10-13')]
    const sessions = [
      { kind: 'work', minutes: 25, started_at: '2026-10-06T08:00:00Z', deleted_at: null },
      { kind: 'break', minutes: 5, started_at: '2026-10-06T08:25:00Z', deleted_at: null },
      { kind: 'work', minutes: 50, started_at: '2026-10-14T08:00:00Z', deleted_at: null },
    ] as FocusSession[]
    expect(weekStats(tasks, entries, sessions, '2026-10-05', '2026-10-12', dateOf)).toEqual({
      done: 3,
      planned: 3,
      plannedDone: 1,
      missed: 1,
      focusMinutes: 25,
      topProjects: [
        { projectId: 'P', done: 2 },
        { projectId: null, done: 1 },
      ],
    })
  })

  it('finds loose ends, each once with its most pressing reason', () => {
    const now = Date.parse(T0)
    const old = '2026-09-01T00:00:00Z'
    const tasks = [
      task('overdue', { due_date: '2026-10-10', carry_count: 5 }),
      task('carried', { carry_count: 3 }),
      task('stale', { updated_at: old }),
      task('stale but planned', { updated_at: old }),
      task('fresh'),
      task('waiting', { updated_at: old, waiting_since: old }),
      task('routine', { updated_at: old, series_id: 'S' }),
      task('done', { status: 'done', due_date: '2026-10-01' }),
    ]
    const found = looseEnds(tasks, (id) => id === 'stale but planned', '2026-10-15', now)
    expect(found.map((l) => [l.task.id, l.why])).toEqual([
      ['overdue', 'overdue'],
      ['carried', 'carried'],
      ['stale', 'stale'],
    ])
  })

  it('finds projects with nothing left to do, counting subprojects', () => {
    const p = (id: string, extra: Partial<Project> = {}) => ({ id, status: 'active', parent_id: null, archived_at: null, deleted_at: null, ...extra }) as Project
    const projects = [p('empty'), p('busy'), p('parent'), p('child', { parent_id: 'parent' }), p('idea', { status: 'idea' }), p('old', { archived_at: T0 })]
    const tasks = [task('1', { project_id: 'busy' }), task('2', { project_id: 'child' }), task('3', { project_id: 'empty', status: 'done' })]
    const subtree = (id: string) => (id === 'parent' ? ['parent', 'child'] : [id])
    expect(projectsWithoutNextStep(projects, tasks, subtree).map((x) => x.id)).toEqual(['empty'])
  })
})
