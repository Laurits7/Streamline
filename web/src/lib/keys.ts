// Keyboard shortcuts (10.2). Single keys work outside text fields; "g" then a letter goes
// to a page. Ctrl+K (⌘K on a Mac) opens the command palette from anywhere.

export type KeyAction =
  | { kind: 'palette' }
  | { kind: 'new' }
  | { kind: 'go'; to: string }
  | { kind: 'move'; by: 1 | -1 }
  | { kind: 'complete' }
  | { kind: 'today' }
  | { kind: 'focus' }
  | { kind: 'help' }

/** "g" then one of these: where it goes ("plan" = today's planning page). */
export const GO: Record<string, { to: string; label: string }> = {
  t: { to: '/', label: 'Today' },
  i: { to: '/inbox', label: 'Inbox' },
  l: { to: '/tasks', label: 'All tasks' },
  p: { to: 'plan', label: 'Plan the day' },
  w: { to: '/week', label: 'Week' },
  a: { to: '/agenda', label: 'Agenda' },
  o: { to: '/projects', label: 'Projects' },
  r: { to: '/routines', label: 'Routines' },
  f: { to: '/focus', label: 'Focus' },
  m: { to: '/tracking', label: 'Tracking' },
  j: { to: '/summary', label: 'Summary & journal' },
  y: { to: '/trends', label: 'Trends' },
  g: { to: '/goals', label: 'Goals' },
  s: { to: '/settings', label: 'Settings' },
  h: { to: '/help', label: 'Help' },
}

/** For the help overlay. */
export const SHORTCUTS: [string, string][] = [
  ['Ctrl K', 'Search and commands (⌘K on a Mac)'],
  ['/', 'Search and commands'],
  ['n', 'New task'],
  ['j / k', 'Next / previous task in the list'],
  ['x', 'Complete the selected task (again to reopen)'],
  ['t', 'Plan the selected task for today'],
  ['f', 'Focus on the selected task'],
  ['Enter', 'Open the selected task'],
  ...Object.entries(GO).map(([k, g]): [string, string] => [`g ${k}`, `Go to ${g.label}`]),
  ['?', 'This list'],
]

const SINGLE: Record<string, KeyAction> = {
  '/': { kind: 'palette' },
  n: { kind: 'new' },
  j: { kind: 'move', by: 1 },
  k: { kind: 'move', by: -1 },
  x: { kind: 'complete' },
  t: { kind: 'today' },
  f: { kind: 'focus' },
  '?': { kind: 'help' },
}

/** How long after "g" the second key counts. */
const SEQUENCE_MS = 1500

/**
 * A key reader: give it each key press (outside text fields, without Ctrl/Alt/⌘) and the
 * time; it answers what to do, or null.
 */
export function keyReader() {
  let g: number | null = null
  return (key: string, now: number): KeyAction | null => {
    if (g !== null && now - g <= SEQUENCE_MS) {
      g = null
      const go = GO[key]
      return go ? { kind: 'go', to: go.to } : null
    }
    g = null
    if (key === 'g') {
      g = now
      return null
    }
    return SINGLE[key] ?? null
  }
}

/** Whether a key press happens while typing (shortcuts stay out of the way then). */
export function typing(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  if (!el || !el.tagName) return false
  return el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)
}
