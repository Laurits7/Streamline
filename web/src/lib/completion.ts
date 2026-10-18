// When a task can be done and when it was done: routines aren't done before their day
// (D-110), and a done task can be dated on an earlier day (D-111).

import type { Task } from './api/types/Task'
import { localParts, zonedToUtc } from './waiting'

type Dated = Pick<Task, 'series_id' | 'occurrence_date' | 'due_date'>

/** A routine occurrence whose day (and due date, if moved earlier) is still to come. */
export function tooEarly(t: Dated, today: string): boolean {
  return !!t.series_id && !!t.occurrence_date && t.occurrence_date > today && (!t.due_date || t.due_date > today)
}

/** The first day a routine occurrence can be done. */
export function doableFrom(t: Dated): string {
  return [t.occurrence_date, t.due_date].filter((d): d is string => !!d).sort()[0] ?? ''
}

const nextDay = (date: string) => {
  const [y, m, d] = date.split('-').map(Number)
  return new Date(Date.UTC(y, m - 1, d + 1)).toISOString().slice(0, 10)
}

/** The instant at `hhmm` on logical day `date`: times before the day's end belong to the next calendar day. */
function onLogicalDay(date: string, hhmm: string, timeZone: string, dayEnd: string): string {
  return zonedToUtc(hhmm < dayEnd ? nextDay(date) : date, hhmm, timeZone)
}

/**
 * When a task done on logical day `date` counts as done: at the time of day it was done
 * (`prev`, if any) or at noon, but never in the future (then noon, else now).
 */
export function doneAtOn(date: string, timeZone: string, dayEnd: string, nowMs: number, prev?: string | null): string {
  const now = new Date(nowMs).toISOString()
  const times = [prev ? localParts(prev, timeZone).time : null, '12:00'].filter((x): x is string => !!x)
  for (const hhmm of times) {
    const at = onLogicalDay(date, hhmm, timeZone, dayEnd)
    if (at <= now) return at
  }
  return now
}
