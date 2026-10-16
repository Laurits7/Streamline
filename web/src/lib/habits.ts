// Habit goals (D-105): a week's answers as a strip, for the weekly review and the Goals page.
import type { Goal } from './api/types/Goal'
import type { HabitCheck } from './api/types/HabitCheck'

export type Answer = HabitCheck['result']
export type StripDay = { date: string; answer: Answer | null; inPeriod: boolean; future: boolean }

const DAY = 864e5
const add = (d: string, n: number) => new Date(Date.parse(`${d}T00:00:00Z`) + n * DAY).toISOString().slice(0, 10)

/** The 7 days from `from`, each with its answer (if any), whether the habit applied, and whether it's still ahead. */
export function weekStrip(goal: Goal, checks: Iterable<HabitCheck>, from: string, today: string): StripDay[] {
  const by = new Map<string, Answer>()
  for (const c of checks) if (c.goal_id === goal.id && !c.deleted_at) by.set(c.date, c.result)
  return Array.from({ length: 7 }, (_, i) => {
    const date = add(from, i)
    return {
      date,
      answer: by.get(date) ?? null,
      inPeriod: (!goal.start_date || goal.start_date <= date) && (!goal.target_date || date <= goal.target_date),
      future: date > today,
    }
  })
}

/** "4 of 6 days good, 1 partly" for a strip (only days of its period that have come). */
export function stripSummary(days: StripDay[]): string {
  const counted = days.filter((d) => d.inPeriod && !d.future)
  if (!counted.length) return 'not started yet'
  const yes = counted.filter((d) => d.answer === 'yes').length
  const partly = counted.filter((d) => d.answer === 'partly').length
  const open = counted.filter((d) => !d.answer).length
  return [`${yes} of ${counted.length} day${counted.length === 1 ? '' : 's'} good`, partly ? `${partly} partly` : '', open ? `${open} not answered` : '']
    .filter(Boolean)
    .join(', ')
}

/** The verdict the week's answers suggest (most good days: yes; mostly bad: no). */
export function suggestedVerdict(days: StripDay[]): Answer | null {
  const answered = days.filter((d) => d.inPeriod && d.answer)
  if (!answered.length) return null
  const score = answered.reduce((s, d) => s + (d.answer === 'yes' ? 1 : d.answer === 'partly' ? 0.5 : 0), 0) / answered.length
  return score >= 0.75 ? 'yes' : score >= 0.4 ? 'partly' : 'no'
}
