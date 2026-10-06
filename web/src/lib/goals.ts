// Goals review timing. Mirror of crates/domain/src/goals.rs `period_start` / `review_due`.
export type Cadence = 'off' | 'weekly' | 'monthly'

const addDays = (date: string, n: number) => {
  const [y, m, d] = date.split('-').map(Number)
  return new Date(Date.UTC(y, m - 1, d + n)).toISOString().slice(0, 10)
}

/** First day of the current review period (null when reviews are off). */
export function periodStart(cadence: Cadence, today: string, weekStart: number): string | null {
  if (cadence === 'off') return null
  if (cadence === 'monthly') return today.slice(0, 8) + '01'
  const iso = ((new Date(today + 'T00:00:00Z').getUTCDay() + 6) % 7) + 1
  const ws = Math.min(7, Math.max(1, weekStart))
  return addDays(today, -((iso + 7 - ws) % 7))
}

export function reviewDue(cadence: Cadence, lastReview: string | null, today: string, weekStart: number): boolean {
  const start = periodStart(cadence, today, weekStart)
  return start !== null && (lastReview === null || lastReview < start)
}
