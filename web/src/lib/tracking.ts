// Tracking helpers (SPEC §6.2b). `daily` and `trend` mirror crates/domain/src/tracking.rs
// so charts update instantly from the local store.
import type { MetricEntry } from './api/types/MetricEntry'

export type Aggregate = 'latest' | 'average' | 'sum' | 'max'
export type Range = 'week' | 'month' | 'year'
export type Point = { start: string; value: number | null }

export const MOODS = ['😞', '🙁', '😐', '🙂', '😄']
export const KG_PER_LB = 0.45359237

/** One value per date (`YYYY-MM-DD`). */
export function daily(entries: MetricEntry[], agg: Aggregate): Map<string, number> {
  const byDay = new Map<string, MetricEntry[]>()
  for (const e of entries) {
    if (e.deleted_at) continue
    const list = byDay.get(e.date)
    if (list) list.push(e)
    else byDay.set(e.date, [e])
  }
  const out = new Map<string, number>()
  for (const [d, es] of byDay) {
    es.sort((a, b) => (a.at < b.at ? -1 : a.at > b.at ? 1 : 0))
    const vals = es.map((e) => e.value)
    out.set(
      d,
      agg === 'latest'
        ? vals[vals.length - 1]
        : agg === 'average'
          ? vals.reduce((a, b) => a + b, 0) / vals.length
          : agg === 'sum'
            ? vals.reduce((a, b) => a + b, 0)
            : Math.max(...vals),
    )
  }
  return out
}

const addDays = (date: string, n: number) => {
  const [y, m, d] = date.split('-').map(Number)
  return new Date(Date.UTC(y, m - 1, d + n)).toISOString().slice(0, 10)
}

/** Points ending on `end`: 7 or 30 days, or 52 weekly averages. */
export function trend(values: Map<string, number>, end: string, range: Range): Point[] {
  const [count, step] = range === 'week' ? [7, 1] : range === 'month' ? [30, 1] : [52, 7]
  const first = addDays(end, -(count * step - 1))
  return Array.from({ length: count }, (_, i) => {
    const start = addDays(first, i * step)
    const vals: number[] = []
    for (let d = 0; d < step; d++) {
      const v = values.get(addDays(start, d))
      if (v !== undefined) vals.push(v)
    }
    return { start, value: vals.length ? vals.reduce((a, b) => a + b, 0) / vals.length : null }
  })
}

/** Weight for display: kg → lb for imperial users (stored values are kg). */
export const showWeight = (kg: number, imperial: boolean) => (imperial ? kg / KG_PER_LB : kg)
export const storeWeight = (shown: number, imperial: boolean) => (imperial ? shown * KG_PER_LB : shown)

export const moodEmoji = (v: number) => MOODS[Math.min(4, Math.max(0, Math.round(v) - 1))]
