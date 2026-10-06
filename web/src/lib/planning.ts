// Planning-ritual helpers (SPEC §6.2d). `freeMinutes` and `pastInLogicalDay` mirror
// crates/domain/src/planning.rs so the wizard can update instantly; the server
// computes the same numbers for API clients (`DayView.free_min`/`planned_min`).
import type { DayEntry } from './api/types/DayEntry'
import type { Me } from './api/types/Me'
import type { Task } from './api/types/Task'
import { addDays } from './dates'

const toMin = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3, 5))

/** Free minutes in [start, end) minus busy `[startHHMM, minutes]` intervals (overlaps count once). */
export function freeMinutes(start: string, end: string, busy: [string, number][]): number {
  const ws = toMin(start)
  let we = toMin(end)
  if (we <= ws) we += 1440
  const spans = busy
    .map(([t, dur]): [number, number] => {
      let s = toMin(t)
      if (s < ws && we > 1440) s += 1440
      return [Math.max(s, ws), Math.min(s + dur, we)]
    })
    .filter(([s, e]) => s < e)
    .sort((a, b) => a[0] - b[0] || a[1] - b[1])
  let busyTotal = 0
  let cur: [number, number] | null = null
  for (const [s, e] of spans) {
    if (cur && s <= cur[1]) cur[1] = Math.max(cur[1], e)
    else {
      if (cur) busyTotal += cur[1] - cur[0]
      cur = [s, e]
    }
  }
  if (cur) busyTotal += cur[1] - cur[0]
  return we - ws - busyTotal
}

/** Has local time `now` reached `time` within the current logical day (which starts at `dayEnd`)? */
export function pastInLogicalDay(now: string, time: string, dayEnd: string): boolean {
  const rel = (t: string) => (toMin(t) - toMin(dayEnd) + 1440) % 1440
  return rel(now) >= rel(time)
}

export type PlanPrompt = { date: string; kind: 'evening' | 'morning' | 'unplanned' }

/** What planning to suggest right now, most relevant first (null = nothing to do). */
export function planPrompt(me: Me, today: string, now: string, isPlanned: (d: string) => boolean): PlanPrompt | null {
  const tomorrow = addDays(today, 1)
  if (me.plan_mode !== 'morning' && pastInLogicalDay(now, me.plan_time_evening, me.day_end) && !isPlanned(tomorrow))
    return { date: tomorrow, kind: 'evening' }
  if (me.plan_mode !== 'evening' && pastInLogicalDay(now, me.plan_time_morning, me.day_end) && !isPlanned(today))
    return { date: today, kind: 'morning' }
  if (!isPlanned(today)) return { date: today, kind: 'unplanned' }
  return null
}

/** Mirror of domain::planning::remaining_window: what's left of the window at `now` (null = over). */
export function remainingWindow(start: string, end: string, now: string): [string, string] | null {
  if (toMin(end) <= toMin(start)) return [start, end]
  if (toMin(now) >= toMin(end)) return null
  return [toMin(now) > toMin(start) ? now : start, end]
}

export type DayLoad = { free: number; planned: number; scheduled: number; unestimated: number }

/**
 * Free time and planned time of a day from its entries (open tasks only for "planned").
 * Pass `now` (HH:MM) for today, so only the rest of the day counts as free, and busy
 * calendar events as `[HH:MM, minutes]` (see calendar.ts `busyIntervals`).
 */
export function dayLoad(
  me: Me,
  entries: DayEntry[],
  task: (id: string) => Task | undefined,
  now?: string,
  events: [string, number][] = [],
): DayLoad {
  const minutes = (e: DayEntry, fallback: number) => e.duration_min ?? task(e.task_id)?.estimate_min ?? fallback
  const busy = entries.filter((e) => e.start_time).map((e): [string, number] => [e.start_time!, minutes(e, 30)])
  let planned = 0
  let unestimated = 0
  for (const e of entries) {
    const t = task(e.task_id)
    if (e.start_time || t?.status !== 'open') continue
    if (e.duration_min === null && t.estimate_min === null) unestimated++
    planned += minutes(e, 0)
  }
  const window = now ? remainingWindow(me.day_window_start, me.day_window_end, now) : [me.day_window_start, me.day_window_end]
  return {
    free: window ? freeMinutes(window[0], window[1], [...busy, ...events]) : 0,
    planned,
    scheduled: busy.length,
    unestimated,
  }
}
