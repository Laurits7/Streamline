// Tracking helpers (SPEC §6.2b). `daily` and `trend` mirror crates/domain/src/tracking.rs
// so charts update instantly from the local store.
import type { MetricDefinition } from './api/types/MetricDefinition'
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

const round = (v: number, places: number) => Math.round(v * 10 ** places) / 10 ** places

/** An amount with its unit; 1000 ml and more as litres ("1.3 l"). */
export function formatAmount(v: number, unit: string): string {
  if (unit === 'ml' && Math.abs(v) >= 1000) return `${round(v / 1000, 2)} l`
  return `${round(v, 1)}${unit ? ' ' + unit : ''}`
}

/** A quick amount's button label: "+200 ml", "+1 l", "−1". */
export const presetLabel = (v: number, unit: string) => (v < 0 ? '−' : '+') + formatAmount(Math.abs(v), unit)

type MetricLike = Pick<MetricDefinition, 'key' | 'kind' | 'unit'>

/** The unit a metric is shown in (weight in lb for imperial users; water stays ml). */
export const unitOf = (m: MetricLike, imperial: boolean) => (m.key === 'weight' ? (imperial ? 'lb' : 'kg') : m.unit)

/** A day's value as text: "72.5 kg", "1.3 l", "yes", "4/5". */
export function valueText(m: MetricLike, v: number, imperial: boolean): string {
  if (m.kind === 'yes_no') return v ? 'yes' : 'no'
  if (m.key === 'mood') return `${Math.round(v * 10) / 10}/5`
  return formatAmount(m.key === 'weight' ? showWeight(v, imperial) : v, unitOf(m, imperial))
}

export const MAX_PRESETS = 6

/** Quick amounts typed as a list ("100, 200 1000"): numbers, not zero, at most 6. */
export function parsePresets(text: string): number[] {
  return text
    .split(/[\s,;]+/)
    .filter(Boolean)
    .map(Number)
    .filter((n) => Number.isFinite(n) && n !== 0 && Math.abs(n) <= 1e6)
    .slice(0, MAX_PRESETS)
}

/** Ready-made metrics to add with one tap in Settings (D-108). */
export type Suggestion = {
  name: string
  emoji: string
  kind: 'number' | 'scale' | 'yes_no'
  unit?: string
  aggregate?: Aggregate
  scale_min?: number
  scale_max?: number
  presets?: number[]
}
export const SUGGESTIONS: Suggestion[] = [
  { name: 'Sleep', emoji: '😴', kind: 'number', unit: 'h', aggregate: 'sum' },
  { name: 'Steps', emoji: '👟', kind: 'number', unit: 'steps', aggregate: 'sum' },
  { name: 'Coffee', emoji: '☕', kind: 'number', unit: 'cups', aggregate: 'sum', presets: [1] },
  { name: 'Energy', emoji: '⚡', kind: 'scale', scale_min: 1, scale_max: 5 },
  { name: 'Exercise', emoji: '🏃', kind: 'number', unit: 'min', aggregate: 'sum', presets: [15, 30, 60] },
  { name: 'Took vitamins', emoji: '💊', kind: 'yes_no' },
]
