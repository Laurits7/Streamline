// Dates are local calendar days as 'YYYY-MM-DD' strings; the server decides "today".
// Formatting follows the user's locale setting (empty = the browser's language).

import { store } from './store.svelte'

/** The user's locale, or undefined for the browser default. Reactive (reads the store). */
export const userLocale = (): string | undefined => store.me?.locale || undefined

export function addDays(date: string, n: number): string {
  const [y, m, d] = date.split('-').map(Number)
  return new Date(Date.UTC(y, m - 1, d + n)).toISOString().slice(0, 10)
}

const utc = (date: string) => new Date(date + 'T00:00:00Z')

export function dayLabel(date: string, today: string): string {
  if (date === today) return 'Today'
  if (date === addDays(today, 1)) return 'Tomorrow'
  if (date === addDays(today, -1)) return 'Yesterday'
  return utc(date).toLocaleDateString(userLocale(), { weekday: 'long', timeZone: 'UTC' })
}

export function longDate(date: string): string {
  return utc(date).toLocaleDateString(userLocale(), { weekday: 'short', day: 'numeric', month: 'long', timeZone: 'UTC' })
}

export function shortDate(date: string, today: string): string {
  if (date === today) return 'Today'
  if (date === addDays(today, 1)) return 'Tomorrow'
  if (date === addDays(today, -1)) return 'Yesterday'
  return utc(date).toLocaleDateString(userLocale(), { day: 'numeric', month: 'short', timeZone: 'UTC' })
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
