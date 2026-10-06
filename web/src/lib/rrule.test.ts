import { describe, expect, it } from 'vitest'
import { describeRule, describeSeries, fromRule, toRule, type Preset } from './rrule'

describe('schedule presets <-> RRULE', () => {
  const cases: [Preset, string][] = [
    [{ kind: 'daily' }, 'FREQ=DAILY'],
    [{ kind: 'weekdays' }, 'FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR'],
    [{ kind: 'weekly', days: ['WE', 'MO'] }, 'FREQ=WEEKLY;BYDAY=MO,WE'],
    [{ kind: 'interval', n: 3 }, 'FREQ=DAILY;INTERVAL=3'],
    [{ kind: 'monthly', day: -1 }, 'FREQ=MONTHLY;BYMONTHDAY=-1'],
    [{ kind: 'yearly', month: 10, day: 3 }, 'FREQ=YEARLY;BYMONTH=10;BYMONTHDAY=3'],
  ]
  it('round-trips the presets', () => {
    for (const [p, rule] of cases) {
      expect(toRule(p)).toBe(rule)
      const back = fromRule(rule)
      expect(toRule(back)).toBe(rule)
    }
  })
  it('keeps anything else as a custom rule', () => {
    expect(fromRule('FREQ=MONTHLY;BYDAY=-1FR')).toEqual({ kind: 'custom', rule: 'FREQ=MONTHLY;BYDAY=-1FR' })
    expect(fromRule('RRULE:freq=daily')).toEqual({ kind: 'daily' })
  })
})

describe('descriptions', () => {
  it('reads naturally', () => {
    expect(describeRule('FREQ=WEEKLY;BYDAY=MO,WE')).toBe('Every Mon, Wed')
    expect(describeRule('FREQ=MONTHLY;BYMONTHDAY=-1')).toBe('Monthly on the last day')
    expect(describeRule('FREQ=MONTHLY;BYMONTHDAY=22')).toBe('Monthly on the 22nd')
    expect(describeRule('FREQ=DAILY;INTERVAL=2')).toBe('Every 2 days')
    expect(describeSeries({ mode: 'anchored', rrule: 'FREQ=DAILY', start_time: '07:30', times_per_window: null, window: null })).toBe('Every day at 07:30')
    expect(describeSeries({ mode: 'flexible', rrule: null, start_time: null, times_per_window: 2, window: 'week' })).toBe('Twice a week')
  })
})
