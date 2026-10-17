import { describe, expect, it } from 'vitest'
import { doableFrom, doneAtOn, tooEarly } from './completion'

describe('routines before their day (D-110)', () => {
  const occ = (occurrence_date: string | null, due_date: string | null, series_id: string | null = 'S1') => ({
    series_id,
    occurrence_date,
    due_date,
  })

  it('refuses a routine occurrence until its day', () => {
    expect(tooEarly(occ('2026-10-13', '2026-10-13'), '2026-10-12')).toBe(true)
    expect(tooEarly(occ('2026-10-13', null), '2026-10-12')).toBe(true)
    expect(tooEarly(occ('2026-10-12', '2026-10-12'), '2026-10-12')).toBe(false)
    expect(tooEarly(occ('2026-10-11', '2026-10-11'), '2026-10-12')).toBe(false)
  })

  it('allows one moved earlier, and anything that is not a routine', () => {
    expect(tooEarly(occ('2026-10-13', '2026-10-12'), '2026-10-12')).toBe(false)
    expect(tooEarly(occ('2026-10-13', '2026-10-13', null), '2026-10-12')).toBe(false)
    expect(tooEarly(occ(null, '2026-10-20'), '2026-10-12')).toBe(false)
  })

  it('names the first day it can be done', () => {
    expect(doableFrom(occ('2026-10-15', '2026-10-14'))).toBe('2026-10-14')
    expect(doableFrom(occ('2026-10-15', null))).toBe('2026-10-15')
  })
})

describe('dating a completion on an earlier day (D-111)', () => {
  const now = Date.parse('2026-10-14T15:00:00.000Z')

  it('keeps the time of day it was done', () => {
    expect(doneAtOn('2026-10-12', 'UTC', '04:00', now, '2026-10-14T09:30:00.000Z')).toBe('2026-10-12T09:30:00.000Z')
    // In Tallinn (UTC+3): 12:30 local.
    expect(doneAtOn('2026-10-12', 'Europe/Tallinn', '04:00', now, '2026-10-14T09:30:00.000Z')).toBe('2026-10-12T09:30:00.000Z')
  })

  it('uses noon without a time', () => {
    expect(doneAtOn('2026-10-12', 'UTC', '04:00', now)).toBe('2026-10-12T12:00:00.000Z')
    expect(doneAtOn('2026-10-12', 'Europe/Tallinn', '00:00', now)).toBe('2026-10-12T09:00:00.000Z')
  })

  it('puts times after midnight but before the day end on the next calendar day', () => {
    expect(doneAtOn('2026-10-12', 'UTC', '04:00', now, '2026-10-14T01:15:00.000Z')).toBe('2026-10-13T01:15:00.000Z')
  })

  it('is never in the future', () => {
    // Today, done at a later time than now: noon, if that has passed.
    expect(doneAtOn('2026-10-14', 'UTC', '04:00', now, '2026-10-13T20:00:00.000Z')).toBe('2026-10-14T12:00:00.000Z')
    // Before noon: now.
    const morning = Date.parse('2026-10-14T08:00:00.000Z')
    expect(doneAtOn('2026-10-14', 'UTC', '04:00', morning)).toBe('2026-10-14T08:00:00.000Z')
  })
})
