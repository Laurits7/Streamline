import { describe, expect, it } from 'vitest'
import type { DayTemplate } from './api/types/DayTemplate'
import type { Series } from './api/types/Series'
import type { WeekTemplate } from './api/types/WeekTemplate'
import { currentRoutine, dayTemplateFor, layOut, occursOn, previewDay, weekFor } from './weekTemplates'

const week = (id: string, mode: WeekTemplate['mode'], tpl: string | null, start: string | null = null, end: string | null = null, position = 'V'): WeekTemplate => ({
  id,
  owner_user_id: 'u',
  name: id,
  days: [tpl, tpl, tpl, tpl, tpl, null, null],
  mode,
  start_date: start,
  end_date: end,
  position,
  created_at: '',
  updated_at: '',
  deleted_at: null,
  rev: 1,
})

describe('layOut', () => {
  it('matches domain::templates::lay_out', () => {
    expect(layOut([480, 720], [], [30, 60, 15])).toEqual([480, 510, 570])
    expect(layOut([480, 720], [[510, 540]], [30, 45])).toEqual([480, 540])
    expect(layOut([480, 600], [], [60, 90, 30])).toEqual([480, null, 540])
    expect(layOut([480, 600], [[0, 1440]], [30])).toEqual([null])
    expect(layOut([480, 600], [[480, 480]], [0])).toEqual([480])
  })
})

describe('weekFor', () => {
  const usual = week('usual', 'default', 'work')
  const b = week('b', 'alternate', 'office', '2026-10-21')
  const hol = { ...week('hol', 'range', null, '2026-12-21', '2027-01-03'), position: 'k' }
  const weeks = [usual, b, hol]
  it('matches domain::templates (alternate from its start week, ranges win)', () => {
    expect(dayTemplateFor('2026-10-19', weeks)).toBe('office')
    expect(dayTemplateFor('2026-10-26', weeks)).toBe('work')
    expect(dayTemplateFor('2026-11-02', weeks)).toBe('office')
    expect(dayTemplateFor('2026-10-12', weeks)).toBe('work')
    expect(dayTemplateFor('2026-10-24', weeks)).toBe(null)
    expect(weekFor('2026-12-21', weeks)?.id).toBe('hol')
    expect(dayTemplateFor('2026-12-21', weeks)).toBe(null)
    expect(weekFor('2027-01-03', weeks)?.id).toBe('hol')
    expect(weekFor('2027-01-04', weeks)?.id).toBe('usual')
    expect(weekFor('2027-01-11', weeks)?.id).toBe('b')
    expect(dayTemplateFor('2026-10-19', [])).toBe(null)
  })
  it('takes the first overlapping range by position and ignores deleted weeks', () => {
    const short = { ...week('short', 'range', 'short', '2026-12-22', '2026-12-23'), position: 'F' }
    expect(dayTemplateFor('2026-12-22', [hol, short, usual])).toBe('short')
    expect(dayTemplateFor('2026-12-24', [hol, short, usual])).toBe(null)
    expect(weekFor('2026-12-22', [{ ...short, deleted_at: 'x' }, usual])?.id).toBe('usual')
  })
})

const routine = (over: Partial<Series>): Series =>
  ({ id: 'r', title: 'Vitamins', mode: 'repeat', rrule: 'FREQ=DAILY', dtstart: '2026-10-01', until: null, estimate_min: 15, split_from: null, created_at: '1', deleted_at: null, ...over }) as Series

describe('occursOn', () => {
  it('reads the schedule presets', () => {
    expect(occursOn(routine({}), '2026-10-20')).toBe(true)
    expect(occursOn(routine({}), '2026-09-30')).toBe(false)
    expect(occursOn(routine({ until: '2026-10-10' }), '2026-10-20')).toBe(false)
    expect(occursOn(routine({ rrule: 'FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR' }), '2026-10-24')).toBe(false)
    expect(occursOn(routine({ rrule: 'FREQ=WEEKLY;BYDAY=SA' }), '2026-10-24')).toBe(true)
    expect(occursOn(routine({ rrule: 'FREQ=DAILY;INTERVAL=2' }), '2026-10-03')).toBe(true)
    expect(occursOn(routine({ rrule: 'FREQ=DAILY;INTERVAL=2' }), '2026-10-04')).toBe(false)
    expect(occursOn(routine({ rrule: 'FREQ=MONTHLY;BYMONTHDAY=-1' }), '2026-10-31')).toBe(true)
    expect(occursOn(routine({ rrule: 'FREQ=MONTHLY;BYMONTHDAY=5' }), '2026-11-05')).toBe(true)
    expect(occursOn(routine({ mode: 'flexible', rrule: null }), '2026-10-20')).toBe(null)
    expect(occursOn(routine({ rrule: 'FREQ=WEEKLY;INTERVAL=2;BYDAY=MO' }), '2026-10-20')).toBe(null)
  })
})

describe('previewDay', () => {
  it('lays out routines due that day and one-off tasks, around what is planned', () => {
    const old = routine({ id: 'old', title: 'Old', deleted_at: null })
    const series = new Map<string, Series>([
      ['old', old],
      ['r2', routine({ id: 'r2', title: 'Vitamins v2', split_from: 'old', created_at: '2' })],
      ['sat', routine({ id: 'sat', title: 'Saturdays', rrule: 'FREQ=WEEKLY;BYDAY=SA' })],
      ['fixed', routine({ id: 'fixed', mode: 'anchored' })],
      ['flex', routine({ id: 'flex', title: 'Laundry', mode: 'flexible', estimate_min: 60 })],
    ])
    expect(currentRoutine('old', series)?.id).toBe('r2')
    const tpl = {
      id: 't',
      blocks: [
        {
          title: 'Deep work',
          start: '09:00',
          end: '10:30',
          energy: 'hard',
          items: [
            { id: 'a', kind: 'routine', series_id: 'old', title: null, estimate_min: null, notes: null },
            { id: 'b', kind: 'routine', series_id: 'sat', title: null, estimate_min: null, notes: null },
            { id: 'c', kind: 'routine', series_id: 'fixed', title: null, estimate_min: null, notes: null },
            { id: 'd', kind: 'task', series_id: null, title: 'Plan', estimate_min: null, notes: null },
            { id: 'e', kind: 'routine', series_id: 'flex', title: null, estimate_min: null, notes: null },
          ],
        },
      ],
    } as unknown as DayTemplate
    const [b] = previewDay(tpl, '2026-10-20', series, [[540, 555]])
    expect(b.items.map((i) => [i.title, i.start, i.maybe])).toEqual([
      ['Vitamins v2', 555, false],
      ['Plan', 570, false],
      ['Laundry', null, true],
    ])
  })
})
