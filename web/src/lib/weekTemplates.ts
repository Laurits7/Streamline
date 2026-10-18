// Week and day templates (D-113), mirrored from `domain::templates`: which day template a
// date gets from the week templates, and where a template block's routines and tasks go.
// The Week view uses it to preview days the server hasn't set up yet.

import type { DayTemplate } from './api/types/DayTemplate'
import type { Series } from './api/types/Series'
import type { TemplateItem } from './api/types/TemplateItem'
import type { WeekTemplate } from './api/types/WeekTemplate'
import { fromRule } from './rrule'

export const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
export const MODE_LABEL: Record<WeekTemplate['mode'], string> = {
  default: 'Usual week',
  alternate: 'Every other week',
  range: 'Dates',
}

export const toMin = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3, 5))
export const hhmm = (m: number) => `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`

const DAY_MS = 86_400_000
const utc = (date: string) => Date.parse(date + 'T00:00:00Z')
/** 0 = Monday … 6 = Sunday. */
export const weekdayIndex = (date: string) => (new Date(utc(date)).getUTCDay() + 6) % 7
const monday = (date: string) => utc(date) - weekdayIndex(date) * DAY_MS

/**
 * Items one after another from the start of a block `[start, end)` (minutes), skipping
 * `busy` intervals; `null` for an item that doesn't fit (later, shorter ones may still).
 */
export function layOut(block: [number, number], busy: [number, number][], durations: number[]): (number | null)[] {
  const taken = busy.filter(([s, e]) => e > s)
  let cursor = block[0]
  return durations.map((d0) => {
    const d = Math.max(1, d0)
    let t = cursor
    for (;;) {
      if (t + d > block[1]) return null
      const ends = taken.filter(([s, e]) => s < t + d && e > t).map(([, e]) => e)
      if (!ends.length) break
      t = Math.max(...ends)
    }
    taken.push([t, t + d])
    cursor = t + d
    return t
  })
}

const byPosition = (a: WeekTemplate, b: WeekTemplate) =>
  a.position < b.position ? -1 : a.position > b.position ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0

/** The week template that decides a date: a range containing it, the alternating week in its weeks, then the default. */
export function weekFor(date: string, weeks: WeekTemplate[]): WeekTemplate | null {
  const live = weeks.filter((w) => !w.deleted_at).sort(byPosition)
  const range = live.find((w) => w.mode === 'range' && !!w.start_date && !!w.end_date && w.start_date <= date && date <= w.end_date)
  if (range) return range
  const alt = live.find((w) => {
    if (w.mode !== 'alternate' || !w.start_date) return false
    const n = Math.round((monday(date) - monday(w.start_date)) / (7 * DAY_MS))
    return n >= 0 && n % 2 === 0
  })
  return alt ?? live.find((w) => w.mode === 'default') ?? null
}

/** The day template (id) a date gets, or null for no blocks. */
export function dayTemplateFor(date: string, weeks: WeekTemplate[]): string | null {
  return weekFor(date, weeks)?.days[weekdayIndex(date)] ?? null
}

const RRULE_DAYS = ['MO', 'TU', 'WE', 'TH', 'FR', 'SA', 'SU']

/**
 * Whether a routine has an occurrence on a date, as far as the client can tell: `null` when
 * it can't (custom rules; an N-per-week routine depends on what's left of its window).
 */
export function occursOn(s: Pick<Series, 'mode' | 'rrule' | 'dtstart' | 'until'>, date: string): boolean | null {
  if (date < s.dtstart || (s.until && date > s.until)) return false
  if (s.mode === 'flexible') return null
  const p = fromRule(s.rrule)
  const [, m, d] = date.split('-').map(Number)
  switch (p.kind) {
    case 'daily':
      return true
    case 'weekdays':
      return weekdayIndex(date) < 5
    case 'weekly':
      return p.days.includes(RRULE_DAYS[weekdayIndex(date)])
    case 'interval':
      return Math.round((utc(date) - utc(s.dtstart)) / DAY_MS) % p.n === 0
    case 'monthly': {
      if (p.day > 0) return d === p.day
      const next = new Date(utc(date) + DAY_MS)
      return next.getUTCDate() === 1
    }
    case 'yearly':
      return m === p.month && d === p.day
    case 'custom':
      return null
  }
}

/** The routine that carries on from this one now (its newest version), if any is left. */
export function currentRoutine(id: string | null, series: Map<string, Series>): Series | null {
  let cur = id ? series.get(id) : undefined
  for (let i = 0; cur && i < 50; i++) {
    const next = [...series.values()].filter((s) => s.split_from === cur!.id).sort((a, b) => b.created_at.localeCompare(a.created_at))[0]
    if (!next) break
    cur = next
  }
  return cur && !cur.deleted_at ? cur : null
}

export type PreviewItem = {
  id: string
  kind: TemplateItem['kind']
  title: string
  /** Minutes from midnight, or null: planned without a time (it doesn't fit). */
  start: number | null
  dur: number
  /** Only if the routine has a slot left (N per week/month) or the client can't tell. */
  maybe: boolean
}
export type PreviewBlock = { key: string; title: string; start: number; end: number; energy: string | null; items: PreviewItem[] }

/** What a day template will put on a date: its blocks, and in them the routines due that day and the one-off tasks. */
export function previewDay(tpl: DayTemplate, date: string, series: Map<string, Series>, busy: [number, number][]): PreviewBlock[] {
  const taken = [...busy]
  return tpl.blocks.map((b, i) => {
    const items: Omit<PreviewItem, 'start'>[] = []
    for (const it of b.items) {
      if (it.kind === 'task') {
        items.push({ id: it.id, kind: 'task', title: it.title ?? '', dur: it.estimate_min || 30, maybe: false })
        continue
      }
      const s = currentRoutine(it.series_id, series)
      // Fixed-time routines are planned at their own time.
      if (!s || s.mode === 'anchored') continue
      const on = occursOn(s, date)
      if (on === false) continue
      items.push({ id: it.id, kind: 'routine', title: s.title, dur: s.estimate_min || 30, maybe: on === null })
    }
    const start = toMin(b.start)
    const end = toMin(b.end)
    const times = layOut([start, end], taken, items.map((x) => x.dur))
    times.forEach((t, j) => t !== null && taken.push([t, t + items[j].dur]))
    return { key: `${tpl.id}:${i}`, title: b.title, start, end, energy: b.energy, items: items.map((x, j) => ({ ...x, start: times[j] })) }
  })
}
