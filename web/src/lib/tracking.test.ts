import { describe, expect, it } from 'vitest'
import type { MetricEntry } from './api/types/MetricEntry'
import { daily, moodEmoji, showWeight, storeWeight, trend } from './tracking'

const e = (date: string, at: string, value: number): MetricEntry => ({
  id: at,
  user_id: 'u',
  metric_id: 'm',
  date,
  at,
  value,
  note: '',
  created_at: at,
  updated_at: at,
  deleted_at: null,
  rev: 1,
})

describe('tracking (mirror of domain::tracking)', () => {
  const es = [e('2026-10-05', '2026-10-05T20:00:00Z', 72), e('2026-10-05', '2026-10-05T07:00:00Z', 73), e('2026-10-06', '2026-10-06T07:00:00Z', 72.5)]
  it('combines a day', () => {
    expect(daily(es, 'latest').get('2026-10-05')).toBe(72)
    expect(daily(es, 'average').get('2026-10-05')).toBe(72.5)
    expect(daily(es, 'sum').get('2026-10-05')).toBe(145)
    expect(daily(es, 'max').get('2026-10-05')).toBe(73)
  })
  it('trends', () => {
    const m = new Map([
      ['2026-10-06', 4],
      ['2026-10-04', 2],
    ])
    const w = trend(m, '2026-10-06', 'week')
    expect(w).toHaveLength(7)
    expect(w[0].start).toBe('2026-09-30')
    expect(w[6]).toEqual({ start: '2026-10-06', value: 4 })
    expect(w[5].value).toBeNull()
    const y = trend(m, '2026-10-06', 'year')
    expect(y).toHaveLength(52)
    expect(y[51]).toEqual({ start: '2026-09-30', value: 3 })
  })
  it('units and moods', () => {
    expect(showWeight(storeWeight(160, true), true)).toBeCloseTo(160)
    expect(showWeight(72.5, false)).toBe(72.5)
    expect(moodEmoji(3.4)).toBe('😐')
    expect(moodEmoji(5)).toBe('😄')
  })
})
