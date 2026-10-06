import { describe, expect, it } from 'vitest'
import type { Me } from './api/types/Me'
import { freeMinutes, pastInLogicalDay, planPrompt, remainingWindow } from './planning'

describe('free time (mirror of domain::planning::free_minutes)', () => {
  it('matches the Rust cases', () => {
    expect(freeMinutes('08:00', '22:00', [])).toBe(840)
    expect(freeMinutes('08:00', '22:00', [['10:00', 60]])).toBe(780)
    expect(freeMinutes('08:00', '22:00', [['10:00', 60], ['10:30', 60]])).toBe(750)
    expect(freeMinutes('08:00', '22:00', [['07:00', 90], ['21:30', 60]])).toBe(780)
    expect(freeMinutes('08:00', '22:00', [['06:00', 60]])).toBe(840)
    expect(freeMinutes('08:00', '22:00', [['08:00', 900]])).toBe(0)
    expect(freeMinutes('18:00', '02:00', [['01:00', 30]])).toBe(450)
    expect(freeMinutes('18:00', '02:00', [['23:30', 60]])).toBe(420)
  })
})

describe('remaining part of today (mirror of remaining_window)', () => {
  it('matches the Rust cases', () => {
    expect(remainingWindow('08:00', '22:00', '07:00')).toEqual(['08:00', '22:00'])
    expect(remainingWindow('08:00', '22:00', '14:05')).toEqual(['14:05', '22:00'])
    expect(remainingWindow('08:00', '22:00', '22:00')).toBeNull()
    expect(freeMinutes('14:00', '22:00', [['10:00', 60], ['15:00', 30]])).toBe(450)
  })
})

describe('planning prompts', () => {
  const me = (mode: Me['plan_mode']): Me => ({
    id: 'U', username: 'u', display_name: 'U', is_admin: false, timezone: 'UTC', day_end: '04:00', locale: '', week_start: 1,
    plan_mode: mode, plan_time_evening: '21:00', plan_time_morning: '07:30', day_window_start: '08:00', day_window_end: '22:00',
    prefs: {}, focus_work_min: 25, focus_short_break_min: 5, focus_long_break_min: 15, focus_long_every: 4,
  })
  const none = () => false

  it('treats times after midnight as the same logical day', () => {
    expect(pastInLogicalDay('01:00', '21:00', '04:00')).toBe(true)
    expect(pastInLogicalDay('20:00', '21:00', '04:00')).toBe(false)
    expect(pastInLogicalDay('23:00', '00:30', '04:00')).toBe(false)
  })

  it('evening mode asks to plan tomorrow after the planning time', () => {
    expect(planPrompt(me('evening'), '2026-10-06', '21:05', none)).toEqual({ date: '2026-10-07', kind: 'evening' })
    // Before the evening time, only the gentle "today isn't planned" fallback.
    expect(planPrompt(me('evening'), '2026-10-06', '10:00', none)).toEqual({ date: '2026-10-06', kind: 'unplanned' })
    // Planned tomorrow and today: nothing.
    expect(planPrompt(me('evening'), '2026-10-06', '22:00', () => true)).toBeNull()
  })

  it('morning mode asks to plan today', () => {
    expect(planPrompt(me('morning'), '2026-10-06', '07:45', none)).toEqual({ date: '2026-10-06', kind: 'morning' })
    expect(planPrompt(me('morning'), '2026-10-06', '22:00', (d) => d === '2026-10-06')).toBeNull()
  })
})
