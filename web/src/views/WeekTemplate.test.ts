// The week editor and the Week view's preview render from the store (D-113).
import { flushSync, mount, unmount } from 'svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { DayTemplate } from '../lib/api/types/DayTemplate'
import type { Me } from '../lib/api/types/Me'
import type { Series } from '../lib/api/types/Series'
import type { WeekTemplate } from '../lib/api/types/WeekTemplate'
import WeekCalendar from '../lib/components/WeekCalendar.svelte'
import WeekPlanner from '../lib/components/WeekPlanner.svelte'
import { store } from '../lib/store.svelte'
import WeekTemplateView from './WeekTemplate.svelte'

const ME = { id: 'U1', timezone: 'UTC', locale: '', week_start: 1, day_window_start: '08:00', day_window_end: '22:00', prefs: {} } as unknown as Me
const TS = '2026-10-06T08:00:00.000Z'
const tpl: DayTemplate = {
  id: 'T1',
  owner_user_id: 'U1',
  name: 'Workday',
  weekdays: [1, 2, 3, 4, 5],
  blocks: [
    {
      title: 'Deep work',
      start: '09:00',
      end: '12:00',
      energy: 'hard',
      items: [
        { id: 'I1', kind: 'routine', series_id: 'S1', title: null, estimate_min: null, notes: null },
        { id: 'I2', kind: 'task', series_id: null, title: 'Plan the day', estimate_min: 20, notes: null },
      ],
    },
  ],
  position: 'V',
  created_at: TS,
  updated_at: TS,
  deleted_at: null,
  rev: 1,
}
const week: WeekTemplate = {
  id: 'W1',
  owner_user_id: 'U1',
  name: 'Usual week',
  days: ['T1', 'T1', 'T1', 'T1', 'T1', null, null],
  mode: 'default',
  start_date: null,
  end_date: null,
  position: 'V',
  created_at: TS,
  updated_at: TS,
  deleted_at: null,
  rev: 1,
}
const vitamins = { id: 'S1', title: 'Vitamins', mode: 'repeat', rrule: 'FREQ=DAILY', dtstart: '2026-10-01', until: null, estimate_min: 15, split_from: null, created_at: TS, deleted_at: null, times_per_window: null, window: null, start_time: null } as unknown as Series

function setup() {
  vi.stubGlobal('fetch', vi.fn(() => new Promise(() => {})))
  store.me = ME
  store.today = '2026-10-19'
  store.dayTemplates.set(tpl.id, tpl)
  store.weekTemplates.set(week.id, week)
  store.series.set(vitamins.id, vitamins)
  store.setUpDays = new Set(['2026-10-19'])
}

let app: ReturnType<typeof mount> | null = null
afterEach(() => {
  if (app) unmount(app)
  app = null
  document.body.innerHTML = ''
  vi.unstubAllGlobals()
})

describe('week editor', () => {
  it('shows each weekday with its day template, blocks and items', () => {
    setup()
    app = mount(WeekTemplateView, { target: document.body })
    flushSync()
    const text = document.body.textContent ?? ''
    expect(text).toContain('Usual week')
    expect(document.querySelectorAll('.tblock').length).toBe(5)
    expect(document.querySelector('.tblock')?.textContent).toContain('Vitamins')
    expect(document.querySelector('.tblock')?.textContent).toContain('Plan the day')
    expect(text).toContain('Shared with 4 other days')
    // The routine list beside the grid.
    expect(document.querySelector('.side')?.textContent).toContain('Vitamins')
  })
})

describe('week editor drags', () => {
  const pointer = (type: string, target: EventTarget, y: number) =>
    target.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: 10, clientY: y, button: 0, isPrimary: true, pointerType: 'mouse' }))
  it('draws a new block on empty time and moves one in 15-minute steps', () => {
    setup()
    app = mount(WeekTemplateView, { target: document.body })
    flushSync()
    const col = document.querySelectorAll('.col')[0]
    // 0.8 px per minute: 14:00 to 15:30.
    pointer('pointerdown', col, 14 * 60 * 0.8)
    pointer('pointermove', window, 15 * 60 * 0.8)
    pointer('pointermove', window, 15.5 * 60 * 0.8)
    flushSync()
    expect(document.querySelector('.draft')?.textContent).toBe('14:00–15:30')
    pointer('pointerup', window, 15.5 * 60 * 0.8)
    flushSync()
    const blocks = store.dayTemplates.get('T1')!.blocks
    expect(blocks.map((b) => [b.start, b.end])).toEqual([
      ['09:00', '12:00'],
      ['14:00', '15:30'],
    ])
    // The new block's sheet opened (the menu alternative to dragging).
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()
    // Move "Deep work" down by 37 minutes: snaps to +30.
    const block = document.querySelector('.tblock')!
    pointer('pointerdown', block, 10 * 60 * 0.8)
    pointer('pointermove', window, (10 * 60 + 37) * 0.8)
    pointer('pointerup', window, (10 * 60 + 37) * 0.8)
    flushSync()
    expect(store.dayTemplates.get('T1')!.blocks[0]).toMatchObject({ start: '09:30', end: '12:30' })
    expect(store.dayTemplates.get('T1')!.blocks[0].items.length).toBe(2)
  })
})

describe('week preview', () => {
  const days = Array.from({ length: 7 }, (_, i) => `2026-10-${19 + i}`)
  it('previews days not set up yet in the calendar and the planner', () => {
    setup()
    app = mount(WeekCalendar, { target: document.body, props: { days } })
    flushSync()
    // Monday is set up (no blocks synced here: no preview); Tue–Fri are previewed.
    expect(document.querySelectorAll('.pv-label').length).toBe(4)
    expect(document.querySelectorAll('.band.pv').length).toBe(4)
    expect(document.querySelector('.pv-item')?.textContent).toContain('Vitamins')
    unmount(app)
    app = mount(WeekPlanner, { target: document.body, props: { days } })
    flushSync()
    const rows = [...document.querySelectorAll('.pv-row')].map((r) => r.textContent?.trim())
    expect(rows.slice(0, 2)).toEqual(['09:00Vitamins', '09:15Plan the day'])
  })
})
