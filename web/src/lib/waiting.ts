// Waiting for results (D-70): check-back times are stored in UTC and shown and picked
// in the user's timezone.

import type { Task } from './api/types/Task'

/** Date and HH:MM of an instant in a timezone. */
export function localParts(iso: string, timeZone: string): { date: string; time: string } {
  const p = Object.fromEntries(
    new Intl.DateTimeFormat('en-CA', {
      timeZone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      hourCycle: 'h23',
    })
      .formatToParts(new Date(iso))
      .map((x) => [x.type, x.value]),
  )
  return { date: `${p.year}-${p.month}-${p.day}`, time: `${p.hour}:${p.minute}` }
}

/** The instant (ISO, UTC) at which the clock in `timeZone` shows `date` `hhmm`. */
export function zonedToUtc(date: string, hhmm: string, timeZone: string): string {
  const [y, m, d] = date.split('-').map(Number)
  const [h, mi] = hhmm.split(':').map(Number)
  const wall = Date.UTC(y, m - 1, d, h, mi)
  // The zone's offset at a guess, applied twice to settle across DST changes.
  let t = wall
  for (let i = 0; i < 2; i++) {
    const p = localParts(new Date(t).toISOString(), timeZone)
    const [py, pm, pd] = p.date.split('-').map(Number)
    const [ph, pmi] = p.time.split(':').map(Number)
    t += wall - Date.UTC(py, pm - 1, pd, ph, pmi)
  }
  return new Date(t).toISOString()
}

export type CheckBackChoice = { label: string; at: string | null }

/** Quick choices: in 30 min, 1 h, 2 h, 4 h, tomorrow morning, and no time. */
export function checkBackChoices(nowMs: number, timeZone: string, morning: string): CheckBackChoice[] {
  const later = (min: number) => new Date(Math.ceil((nowMs + min * 60_000) / 60_000) * 60_000).toISOString()
  const { date } = localParts(new Date(nowMs).toISOString(), timeZone)
  const [y, m, d] = date.split('-').map(Number)
  const tomorrow = new Date(Date.UTC(y, m - 1, d + 1)).toISOString().slice(0, 10)
  return [
    { label: 'In 30 min', at: later(30) },
    { label: 'In 1 h', at: later(60) },
    { label: 'In 2 h', at: later(120) },
    { label: 'In 4 h', at: later(240) },
    { label: `Tomorrow ${morning}`, at: zonedToUtc(tomorrow, morning, timeZone) },
    { label: 'No time', at: null },
  ]
}

/** "15:30" today, "tomorrow 08:00", or "12 Oct 08:00". */
export function checkBackLabel(iso: string, timeZone: string, nowMs: number, locale?: string): string {
  const at = localParts(iso, timeZone)
  const now = localParts(new Date(nowMs).toISOString(), timeZone)
  if (at.date === now.date) return at.time
  const [y, m, d] = now.date.split('-').map(Number)
  if (at.date === new Date(Date.UTC(y, m - 1, d + 1)).toISOString().slice(0, 10)) return `tomorrow ${at.time}`
  const day = new Date(at.date + 'T00:00:00Z').toLocaleDateString(locale, { day: 'numeric', month: 'short', timeZone: 'UTC' })
  return `${day} ${at.time}`
}

/** Waiting for results: needs no action until the check-back time. */
export function isWaiting(t: Pick<Task, 'status' | 'waiting_since'>): boolean {
  return t.status === 'open' && !!t.waiting_since
}

/** The check-back time has come: worth a look now. */
export function checkBackDue(t: Pick<Task, 'status' | 'waiting_since' | 'check_back_at'>): boolean {
  return t.status === 'open' && !t.waiting_since && !!t.check_back_at
}
