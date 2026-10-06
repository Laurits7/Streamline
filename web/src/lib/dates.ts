// Dates are local calendar days as 'YYYY-MM-DD' strings; the server decides "today".

export function addDays(date: string, n: number): string {
  const [y, m, d] = date.split('-').map(Number)
  return new Date(Date.UTC(y, m - 1, d + n)).toISOString().slice(0, 10)
}

const utc = (date: string) => new Date(date + 'T00:00:00Z')

export function dayLabel(date: string, today: string): string {
  if (date === today) return 'Today'
  if (date === addDays(today, 1)) return 'Tomorrow'
  if (date === addDays(today, -1)) return 'Yesterday'
  return utc(date).toLocaleDateString(undefined, { weekday: 'long', timeZone: 'UTC' })
}

export function longDate(date: string): string {
  return utc(date).toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'long', timeZone: 'UTC' })
}

export function shortDate(date: string, today: string): string {
  if (date === today) return 'Today'
  if (date === addDays(today, 1)) return 'Tomorrow'
  if (date === addDays(today, -1)) return 'Yesterday'
  return utc(date).toLocaleDateString(undefined, { day: 'numeric', month: 'short', timeZone: 'UTC' })
}

export function fmtMinutes(m: number): string {
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  return m % 60 ? `${h}h ${m % 60}m` : `${h}h`
}

/** Current HH:MM in a timezone. */
export function nowHHMM(timeZone: string): string {
  return new Intl.DateTimeFormat('en-GB', { timeZone, hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(
    new Date(),
  )
}
