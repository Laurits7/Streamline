// Calendar events on a day (SPEC §6.5). Events are stored as UTC instants (timed) or
// dates (all-day); the day view works in the user's local clock time, 00:00–24:00.
import type { Calendar } from './api/types/Calendar'
import type { CalendarEvent } from './api/types/CalendarEvent'

export type DayEvent = {
  event: CalendarEvent
  /** Minutes from local midnight, clipped to the day (0 for all-day). */
  start: number
  /** Minutes on this day (clipped). */
  dur: number
  /** Started on an earlier day / ends on a later one. */
  before: boolean
  after: boolean
  color: string
}

export const DEFAULT_COLOR = '#64748b'

export function calendarColor(c: Calendar | undefined): string {
  return c?.user_color || c?.color || DEFAULT_COLOR
}

const formatters = new Map<string, Intl.DateTimeFormat>()

/** Local date (`YYYY-MM-DD`) and minute of day of an instant in a time zone. */
export function zoned(iso: string, timeZone: string): { date: string; min: number } {
  let f = formatters.get(timeZone)
  if (!f) {
    f = new Intl.DateTimeFormat('en-CA', {
      timeZone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      hourCycle: 'h23',
    })
    formatters.set(timeZone, f)
  }
  const p = Object.fromEntries(f.formatToParts(new Date(iso)).map((x) => [x.type, x.value]))
  return {
    date: `${p.year}-${p.month}-${p.day}`,
    min: Number(p.hour) * 60 + Number(p.minute),
  }
}

const dayNumber = (date: string) => Date.UTC(+date.slice(0, 4), +date.slice(5, 7) - 1, +date.slice(8, 10)) / 86_400_000

/** Minutes from local midnight of `date` to the instant (negative = earlier day). */
function minutesFrom(date: string, iso: string, timeZone: string): number {
  const z = zoned(iso, timeZone)
  return (dayNumber(z.date) - dayNumber(date)) * 1440 + z.min
}

/** The day's events: all-day ones first, then timed ones by start, each clipped to the day. */
export function eventsOn(
  date: string,
  events: Iterable<CalendarEvent>,
  timeZone: string,
  calendar: (id: string) => Calendar | undefined,
): { allDay: DayEvent[]; timed: DayEvent[] } {
  const allDay: DayEvent[] = []
  const timed: DayEvent[] = []
  for (const e of events) {
    const cal = calendar(e.calendar_id)
    if (e.deleted_at || (cal && !cal.enabled)) continue
    const color = calendarColor(cal)
    if (e.all_day) {
      if (e.start_date! <= date && date < e.end_date!)
        allDay.push({
          event: e,
          start: 0,
          dur: 1440,
          before: e.start_date! < date,
          after: e.end_date! > nextDay(date),
          color,
        })
      continue
    }
    const s = minutesFrom(date, e.start_at!, timeZone)
    const end = minutesFrom(date, e.end_at!, timeZone)
    const inDay = end > s ? s < 1440 && end > 0 : s >= 0 && s < 1440
    if (!inDay) continue
    const cs = Math.max(0, s)
    timed.push({
      event: e,
      start: cs,
      dur: Math.min(1440, end) - cs,
      before: s < 0,
      after: end > 1440,
      color,
    })
  }
  allDay.sort((a, b) => a.event.title.localeCompare(b.event.title))
  timed.sort((a, b) => a.start - b.start || b.dur - a.dur)
  return { allDay, timed }
}

function nextDay(date: string): string {
  return new Date((dayNumber(date) + 1) * 86_400_000).toISOString().slice(0, 10)
}

const hhmm = (m: number) => `${String(Math.floor(m / 60) % 24).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`

/** Busy intervals for free-time math: `[HH:MM, minutes]` of busy timed events. */
export function busyIntervals(timed: DayEvent[]): [string, number][] {
  return timed.filter((d) => d.event.busy && d.dur > 0).map((d) => [hhmm(d.start), d.dur])
}

/** "09:30–10:00", with arrows when the event runs over from/into another day. */
export function timeRange(d: DayEvent): string {
  if (d.event.all_day) return 'All day'
  const from = d.before ? '…' : hhmm(d.start)
  const to = d.after ? '…' : hhmm(d.start + d.dur)
  return d.dur === 0 ? from : `${from}–${to}`
}
