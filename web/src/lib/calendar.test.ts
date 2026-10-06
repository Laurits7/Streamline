import { describe, expect, it } from 'vitest'
import type { Calendar } from './api/types/Calendar'
import type { CalendarEvent } from './api/types/CalendarEvent'
import { busyIntervals, eventsOn, timeRange, zoned } from './calendar'

const base: CalendarEvent = {
  id: 'e',
  user_id: 'u',
  calendar_id: 'c',
  uid: 'x',
  instance_key: '',
  title: 'Event',
  location: null,
  all_day: false,
  start_at: null,
  end_at: null,
  start_date: null,
  end_date: null,
  busy: true,
  recurring: false,
  updated_at: '',
  deleted_at: null,
  rev: 1,
}
const timed = (id: string, start: string, end: string, extra: Partial<CalendarEvent> = {}): CalendarEvent => ({
  ...base,
  id,
  title: id,
  start_at: start,
  end_at: end,
  ...extra,
})
const cal: Calendar = {
  id: 'c',
  account_id: 'a',
  user_id: 'u',
  href: '',
  name: 'Work',
  color: '#3b82f6',
  user_color: null,
  enabled: true,
  created_at: '',
  updated_at: '',
  deleted_at: null,
  rev: 1,
}

describe('calendar events on a day', () => {
  it('converts to local time', () => {
    expect(zoned('2026-10-07T06:30:00Z', 'Europe/Tallinn')).toEqual({
      date: '2026-10-07',
      min: 9 * 60 + 30,
    })
    expect(zoned('2026-10-07T02:00:00Z', 'America/New_York')).toEqual({
      date: '2026-10-06',
      min: 22 * 60,
    })
  })

  it('places, clips and colours events', () => {
    const evs = [
      timed('standup', '2026-10-07T06:30:00Z', '2026-10-07T07:00:00Z'),
      timed('overnight', '2026-10-06T19:00:00Z', '2026-10-07T05:00:00Z'), // 22:00 → 08:00 local
      timed('tomorrow', '2026-10-08T06:30:00Z', '2026-10-08T07:00:00Z'),
      timed('free', '2026-10-07T10:00:00Z', '2026-10-07T11:00:00Z', {
        busy: false,
      }),
      timed('point', '2026-10-07T12:00:00Z', '2026-10-07T12:00:00Z'),
      {
        ...base,
        id: 'trip',
        title: 'Trip',
        all_day: true,
        start_date: '2026-10-06',
        end_date: '2026-10-09',
      },
      {
        ...base,
        id: 'gone',
        all_day: true,
        start_date: '2026-10-07',
        end_date: '2026-10-08',
        deleted_at: 'x',
      },
    ]
    const d = eventsOn('2026-10-07', evs, 'Europe/Tallinn', () => cal)
    expect(d.allDay.map((x) => [x.event.id, x.before, x.after])).toEqual([['trip', true, true]])
    expect(d.timed.map((x) => [x.event.id, x.start, x.dur])).toEqual([
      ['overnight', 0, 480],
      ['standup', 570, 30],
      ['free', 780, 60],
      ['point', 900, 0],
    ])
    expect(d.timed[0].color).toBe('#3b82f6')
    expect(timeRange(d.timed[0])).toBe('…–08:00')
    expect(timeRange(d.timed[1])).toBe('09:30–10:00')
    expect(busyIntervals(d.timed)).toEqual([
      ['00:00', 480],
      ['09:30', 30],
    ])
    // Hidden calendars show nothing; the user's colour wins.
    expect(
      eventsOn('2026-10-07', evs, 'Europe/Tallinn', () => ({
        ...cal,
        enabled: false,
      })).timed,
    ).toEqual([])
    expect(
      eventsOn('2026-10-07', evs, 'Europe/Tallinn', () => ({
        ...cal,
        user_color: '#ff0000',
      })).timed[0].color,
    ).toBe('#ff0000')
  })
})
