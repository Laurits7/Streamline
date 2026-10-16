import { describe, expect, it } from 'vitest'
import type { Task } from './api/types/Task'
import { accuracy, factorFor, learn, realistic, samples, suggestion } from './estimates'

let n = 0
const done = (estimate: number, actual: number, extra: Partial<Task> = {}) =>
  ({
    id: `t${++n}`,
    status: 'done',
    deleted_at: null,
    estimate_min: estimate,
    actual_min: actual,
    project_id: null,
    task_type_id: 'tt_carry_on',
    completed_at: `2026-10-${String(1 + (n % 28)).padStart(2, '0')}T10:00:00Z`,
    ...extra,
  }) as Task

describe('better estimates (10.5)', () => {
  it('only learns from finished tasks with an estimate and focus time', () => {
    const tasks = [done(30, 45), done(30, 0), done(0, 20), done(30, 30, { status: 'open' }), done(3, 10)]
    expect(samples(tasks).map((s) => s.ratio)).toEqual([1.5])
  })

  it('takes the median per project, per type and overall, with enough samples', () => {
    const house = { project_id: 'house' }
    const tasks = [
      done(30, 60, house),
      done(30, 45, house),
      done(20, 40, house),
      done(60, 60),
      done(60, 30, { task_type_id: 'tt_deadline' }),
      done(60, 600), // a runaway task barely moves the median
    ]
    const m = learn(tasks)
    expect(m.byProject.get('house')).toEqual({ factor: 2, n: 3 })
    expect(m.byProject.get(null)).toEqual({ factor: 1, n: 3 })
    expect(m.byType.has('tt_deadline')).toBe(false) // one sample isn't enough
    expect(m.overall).toEqual({ factor: 1.75, n: 6 })
    expect(factorFor(m, { project_id: 'house', task_type_id: 'x' })?.basis).toBe('project')
    expect(factorFor(m, { project_id: 'garden', task_type_id: 'tt_carry_on' })).toEqual({ factor: 2, n: 5, basis: 'type' })
    expect(factorFor(m, { project_id: 'garden', task_type_id: 'other' })?.basis).toBe('overall')
    expect(learn(tasks.slice(0, 4)).overall).toBeNull()
  })

  it('suggests a realistic estimate only when clearly off', () => {
    const tasks = Array.from({ length: 5 }, () => done(30, 45, { project_id: 'p' }))
    const m = learn(tasks)
    const open = (estimate: number, project_id = 'p') => ({ estimate_min: estimate, project_id, task_type_id: 'x', status: 'open' }) as Task
    expect(suggestion(m, open(30))).toEqual({ factor: 1.5, n: 5, basis: 'project', minutes: 45 })
    expect(suggestion(m, { ...open(30), estimate_min: null })).toBeNull()
    const fine = learn(Array.from({ length: 5 }, () => done(30, 33)))
    expect(suggestion(fine, open(30, 'q'))).toBeNull()
    expect(realistic(20, 1.3)).toBe(25)
    expect(realistic(5, 0.25)).toBe(5)
  })

  it('caps extreme factors', () => {
    const m = learn(Array.from({ length: 5 }, () => done(5, 300)))
    expect(m.overall?.factor).toBe(4)
  })

  it('measures accuracy over time', () => {
    const tasks = [done(30, 30, { completed_at: '2026-10-05T09:00:00Z' }), done(30, 60, { completed_at: '2026-10-06T09:00:00Z' }), done(60, 50, { completed_at: '2026-10-13T09:00:00Z' })]
    const a = accuracy(tasks, (iso) => (iso < '2026-10-12' ? '2026-10-05' : '2026-10-12'))
    expect(a.n).toBe(3)
    expect(a.within).toBeCloseTo(2 / 3)
    expect(a.weeks).toEqual([
      { week: '2026-10-05', factor: 1.5, n: 2 },
      { week: '2026-10-12', factor: 50 / 60, n: 1 },
    ])
  })
})
