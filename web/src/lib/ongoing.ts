// Ongoing tasks (D-102): kept at over a period ("keep reading Dune") rather than done in
// one go. Marked "worked on it" per day; finished by hand. Mirrors crates/domain/src/ongoing.rs.
import type { Task } from './api/types/Task'

const DAY = 864e5
const toMs = (d: string) => Date.parse(`${d}T00:00:00Z`)
const fromMs = (ms: number) => new Date(ms).toISOString().slice(0, 10)

/** The day its period starts: `start_date`, else the day it was created (UTC date). */
export function periodStart(t: Task): string {
  return t.start_date ?? t.created_at.slice(0, 10)
}

/** Open, ongoing, and its period includes `day`. */
export function onFor(t: Task, day: string): boolean {
  return t.ongoing && t.status === 'open' && !t.deleted_at && periodStart(t) <= day && (!t.due_date || day <= t.due_date)
}

export function workedOn(t: Task, day: string): boolean {
  return t.worked_days.includes(day)
}

/** The days list with `day` marked or unmarked (sorted, no duplicates). */
export function setWorked(days: string[], day: string, worked: boolean): string[] {
  const out = days.filter((d) => d !== day)
  if (worked) out.push(day)
  return [...new Set(out)].sort()
}

export type Progress = {
  days: number
  thisWeek: number
  /** Days of the period so far. */
  periodDays: number
  /** Consecutive days up to today (or yesterday if today isn't marked yet). */
  streak: number
  workedToday: boolean
  /** Past its end date: time to finish it, or give it more time. */
  over: boolean
}

export function progress(t: Task, today: string, weekStart = 1): Progress {
  const days = [...new Set(t.worked_days)].sort()
  const set = new Set(days)
  const dow = new Date(toMs(today)).getUTCDay() || 7 // ISO 1..7
  const offset = (dow + 7 - Math.min(7, Math.max(1, weekStart))) % 7
  const weekFrom = fromMs(toMs(today) - offset * DAY)
  const workedToday = set.has(today)
  let streak = 0
  let ms = toMs(today) - (workedToday ? 0 : DAY)
  while (set.has(fromMs(ms))) {
    streak++
    ms -= DAY
  }
  const start = periodStart(t)
  const upto = t.due_date && t.due_date < today ? t.due_date : today
  return {
    days: days.length,
    thisWeek: days.filter((d) => d >= weekFrom && d <= today).length,
    periodDays: Math.max(0, Math.round((toMs(upto) - toMs(start)) / DAY) + 1),
    streak,
    workedToday,
    over: !!t.due_date && t.due_date < today,
  }
}

/** A short line for lists: "5 days · 3 this week" (or "not yet"). */
export function summary(p: Progress): string {
  if (!p.days) return 'not worked on yet'
  return [`${p.days} day${p.days === 1 ? '' : 's'}`, p.thisWeek ? `${p.thisWeek} this week` : '', p.streak > 1 ? `${p.streak} in a row` : '']
    .filter(Boolean)
    .join(' · ')
}
