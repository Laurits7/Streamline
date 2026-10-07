import { describe, expect, it } from 'vitest'
import { checkBackChoices, checkBackDue, checkBackLabel, isWaiting, localParts, zonedToUtc } from './waiting'

describe('check-back times', () => {
  it('converts wall-clock times in a zone to UTC, across DST', () => {
    expect(zonedToUtc('2026-10-07', '08:00', 'Europe/Tallinn')).toBe('2026-10-07T05:00:00.000Z')
    expect(zonedToUtc('2026-12-07', '08:00', 'Europe/Tallinn')).toBe('2026-12-07T06:00:00.000Z')
    expect(zonedToUtc('2026-10-25', '12:00', 'Europe/Tallinn')).toBe('2026-10-25T10:00:00.000Z')
    expect(zonedToUtc('2026-10-07', '08:00', 'UTC')).toBe('2026-10-07T08:00:00.000Z')
    expect(localParts('2026-10-07T21:30:00.000Z', 'Europe/Tallinn')).toEqual({ date: '2026-10-08', time: '00:30' })
  })

  it('offers quick choices', () => {
    const now = Date.parse('2026-10-07T12:00:20.000Z')
    const c = checkBackChoices(now, 'Europe/Tallinn', '08:00')
    expect(c.map((x) => x.label)).toEqual(['In 30 min', 'In 1 h', 'In 2 h', 'In 4 h', 'Tomorrow 08:00', 'No time'])
    expect(c[0].at).toBe('2026-10-07T12:31:00.000Z')
    expect(c[4].at).toBe('2026-10-08T05:00:00.000Z')
    expect(c[5].at).toBeNull()
  })

  it('labels times relative to today', () => {
    const now = Date.parse('2026-10-07T12:00:00.000Z')
    expect(checkBackLabel('2026-10-07T13:30:00.000Z', 'UTC', now)).toBe('13:30')
    expect(checkBackLabel('2026-10-08T08:00:00.000Z', 'UTC', now)).toBe('tomorrow 08:00')
    expect(checkBackLabel('2026-10-12T08:00:00.000Z', 'UTC', now, 'en-GB')).toBe('12 Oct 08:00')
  })

  it('tells waiting from due', () => {
    expect(isWaiting({ status: 'open', waiting_since: 'x' })).toBe(true)
    expect(isWaiting({ status: 'done', waiting_since: 'x' })).toBe(false)
    expect(checkBackDue({ status: 'open', waiting_since: null, check_back_at: 'x' })).toBe(true)
    expect(checkBackDue({ status: 'open', waiting_since: 'x', check_back_at: 'x' })).toBe(false)
  })
})
