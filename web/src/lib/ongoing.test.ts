import { describe, expect, it } from 'vitest'
import type { Task } from './api/types/Task'
import { onFor, progress, setWorked, summary } from './ongoing'

const task = (over: Partial<Task> = {}): Task =>
  ({
    id: 't',
    ongoing: true,
    status: 'open',
    deleted_at: null,
    start_date: null,
    due_date: null,
    created_at: '2026-10-01T09:00:00.000Z',
    worked_days: [],
    ...over,
  }) as Task

describe('ongoing tasks', () => {
  it('marks days once, in order', () => {
    let d = setWorked([], '2026-10-15', true)
    d = setWorked(d, '2026-10-13', true)
    d = setWorked(d, '2026-10-15', true)
    expect(d).toEqual(['2026-10-13', '2026-10-15'])
    expect(setWorked(d, '2026-10-13', false)).toEqual(['2026-10-15'])
  })

  it('is on for the days of its period', () => {
    expect(onFor(task(), '2026-10-15')).toBe(true)
    expect(onFor(task(), '2026-09-30')).toBe(false) // before it was created
    expect(onFor(task({ start_date: '2026-10-20' }), '2026-10-15')).toBe(false)
    expect(onFor(task({ due_date: '2026-10-14' }), '2026-10-15')).toBe(false)
    expect(onFor(task({ status: 'done' }), '2026-10-15')).toBe(false)
    expect(onFor(task({ ongoing: false }), '2026-10-15')).toBe(false)
  })

  it('counts progress like the server does', () => {
    // Thursday 15 October 2026; weeks start on Monday the 12th.
    const t = task({ worked_days: ['2026-10-08', '2026-10-12', '2026-10-13', '2026-10-14'], due_date: '2026-10-31' })
    const p = progress(t, '2026-10-15', 1)
    expect(p).toEqual({ days: 4, thisWeek: 3, periodDays: 15, streak: 3, workedToday: false, over: false })
    expect(summary(p)).toBe('4 days · 3 this week · 3 in a row')
    const sunday = progress({ ...t, worked_days: [...t.worked_days, '2026-10-15'] }, '2026-10-15', 7)
    expect(sunday.thisWeek).toBe(4)
    expect(sunday.streak).toBe(4)
    expect(progress(task({ due_date: '2026-10-10' }), '2026-10-15').over).toBe(true)
    expect(summary(progress(task(), '2026-10-15'))).toBe('not worked on yet')
  })
})
