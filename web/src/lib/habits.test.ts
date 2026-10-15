import { describe, expect, it } from 'vitest'
import type { Goal } from './api/types/Goal'
import type { HabitCheck } from './api/types/HabitCheck'
import { stripSummary, suggestedVerdict, weekStrip } from './habits'

const goal = { id: 'g', kind: 'habit', start_date: '2026-10-13', target_date: null } as unknown as Goal
const check = (date: string, result: HabitCheck['result']) => ({ goal_id: 'g', date, result, deleted_at: null }) as HabitCheck

describe('habit week strip', () => {
  const checks = [check('2026-10-13', 'yes'), check('2026-10-14', 'partly'), check('2026-10-15', 'yes'), check('2026-10-01', 'no')]
  const strip = weekStrip(goal, checks, '2026-10-12', '2026-10-16')

  it('lays out the week', () => {
    expect(strip.map((d) => d.answer)).toEqual([null, 'yes', 'partly', 'yes', null, null, null])
    expect(strip[0].inPeriod).toBe(false) // before it started
    expect(strip[5].future).toBe(true)
  })

  it('sums it up', () => {
    expect(stripSummary(strip)).toBe('2 of 4 days good, 1 partly, 1 not answered')
    expect(suggestedVerdict(strip)).toBe('yes') // (1 + .5 + 1) / 3
    expect(suggestedVerdict(weekStrip(goal, [check('2026-10-13', 'no'), check('2026-10-14', 'partly')], '2026-10-12', '2026-10-16'))).toBe('no')
    expect(suggestedVerdict(weekStrip(goal, [], '2026-10-12', '2026-10-16'))).toBe(null)
    expect(stripSummary(weekStrip(goal, [], '2026-10-05', '2026-10-16'))).toBe('not started yet')
  })
})
