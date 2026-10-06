// The routine editor's schedule presets <-> RRULE strings, and plain-language
// descriptions. Anything the presets can't express stays a raw ("custom") rule.
import type { Series } from './api/types/Series'

export const DAYS = ['MO', 'TU', 'WE', 'TH', 'FR', 'SA', 'SU'] as const
export const DAY_NAMES: Record<string, string> = { MO: 'Mon', TU: 'Tue', WE: 'Wed', TH: 'Thu', FR: 'Fri', SA: 'Sat', SU: 'Sun' }
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

export type Preset =
  | { kind: 'daily' }
  | { kind: 'weekdays' }
  | { kind: 'weekly'; days: string[] }
  | { kind: 'interval'; n: number }
  | { kind: 'monthly'; day: number } // -1 = last day
  | { kind: 'yearly'; month: number; day: number }
  | { kind: 'custom'; rule: string }

export function toRule(p: Preset): string {
  switch (p.kind) {
    case 'daily':
      return 'FREQ=DAILY'
    case 'weekdays':
      return 'FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR'
    case 'weekly':
      return `FREQ=WEEKLY;BYDAY=${DAYS.filter((d) => p.days.includes(d)).join(',') || 'MO'}`
    case 'interval':
      return `FREQ=DAILY;INTERVAL=${Math.max(1, Math.round(p.n))}`
    case 'monthly':
      return `FREQ=MONTHLY;BYMONTHDAY=${p.day}`
    case 'yearly':
      return `FREQ=YEARLY;BYMONTH=${p.month};BYMONTHDAY=${p.day}`
    case 'custom':
      return p.rule.trim().replace(/^RRULE:/i, '')
  }
}

export function fromRule(rule: string | null | undefined): Preset {
  const r = (rule ?? '').trim().toUpperCase().replace(/^RRULE:/, '')
  const parts = Object.fromEntries(r.split(';').filter(Boolean).map((kv) => kv.split('=') as [string, string]))
  const keys = Object.keys(parts).sort().join(',')
  if (r === 'FREQ=DAILY') return { kind: 'daily' }
  if (keys === 'FREQ,INTERVAL' && parts.FREQ === 'DAILY' && /^\d+$/.test(parts.INTERVAL))
    return { kind: 'interval', n: Number(parts.INTERVAL) }
  if (keys === 'BYDAY,FREQ' && parts.FREQ === 'WEEKLY') {
    const days = parts.BYDAY.split(',')
    if (days.every((d) => (DAYS as readonly string[]).includes(d))) {
      if (days.length === 5 && ['MO', 'TU', 'WE', 'TH', 'FR'].every((d) => days.includes(d))) return { kind: 'weekdays' }
      return { kind: 'weekly', days }
    }
  }
  if (keys === 'BYMONTHDAY,FREQ' && parts.FREQ === 'MONTHLY' && /^-?\d+$/.test(parts.BYMONTHDAY))
    return { kind: 'monthly', day: Number(parts.BYMONTHDAY) }
  if (keys === 'BYMONTH,BYMONTHDAY,FREQ' && parts.FREQ === 'YEARLY')
    return { kind: 'yearly', month: Number(parts.BYMONTH), day: Number(parts.BYMONTHDAY) }
  return { kind: 'custom', rule: r }
}

const ordinal = (n: number) => `${n}${n % 10 === 1 && n !== 11 ? 'st' : n % 10 === 2 && n !== 12 ? 'nd' : n % 10 === 3 && n !== 13 ? 'rd' : 'th'}`

export function describeRule(rule: string | null | undefined): string {
  const p = fromRule(rule)
  switch (p.kind) {
    case 'daily':
      return 'Every day'
    case 'weekdays':
      return 'Weekdays'
    case 'weekly':
      return p.days.length === 7 ? 'Every day' : `Every ${DAYS.filter((d) => p.days.includes(d)).map((d) => DAY_NAMES[d]).join(', ')}`
    case 'interval':
      return p.n === 1 ? 'Every day' : `Every ${p.n} days`
    case 'monthly':
      return p.day === -1 ? 'Monthly on the last day' : `Monthly on the ${ordinal(p.day)}`
    case 'yearly':
      return `Every year on ${p.day} ${MONTHS[p.month - 1] ?? '?'}`
    case 'custom':
      return p.rule || 'Custom'
  }
}

/** "Every Mon, Wed at 07:30", "2× per week", ... */
export function describeSeries(s: Pick<Series, 'mode' | 'rrule' | 'start_time' | 'times_per_window' | 'window'>): string {
  if (s.mode === 'flexible') {
    const n = s.times_per_window ?? 1
    return `${n === 1 ? 'Once' : n === 2 ? 'Twice' : `${n}×`} a ${s.window ?? 'week'}`
  }
  const base = describeRule(s.rrule)
  return s.mode === 'anchored' && s.start_time ? `${base} at ${s.start_time}` : base
}
