import { describe, expect, it } from 'vitest'
import { periodStart, reviewDue } from './goals'

describe('goal reviews (mirror of domain::goals)', () => {
  it('periods', () => {
    expect(periodStart('weekly', '2026-10-07', 1)).toBe('2026-10-05')
    expect(periodStart('weekly', '2026-10-07', 7)).toBe('2026-10-04')
    expect(periodStart('monthly', '2026-10-07', 1)).toBe('2026-10-01')
    expect(periodStart('off', '2026-10-07', 1)).toBeNull()
  })
  it('due once per period', () => {
    expect(reviewDue('weekly', null, '2026-10-07', 1)).toBe(true)
    expect(reviewDue('weekly', '2026-10-04', '2026-10-07', 1)).toBe(true)
    expect(reviewDue('weekly', '2026-10-05', '2026-10-07', 1)).toBe(false)
    expect(reviewDue('monthly', '2026-10-31', '2026-11-01', 1)).toBe(true)
    expect(reviewDue('off', null, '2026-10-07', 1)).toBe(false)
  })
})
