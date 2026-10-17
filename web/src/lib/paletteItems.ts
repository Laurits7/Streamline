// What the command palette lists (10.2), from what the app has loaded (so it works offline
// in the desktop app too).

import { longDate } from './dates'
import type { PaletteItem } from './palette'
import { store } from './store.svelte'
import { ui } from './ui.svelte'

export type Entry = PaletteItem & { href?: string; run?: () => void; taskId?: string }

const PAGES: [string, string][] = [
  ['Today', '/'],
  ['Inbox', '/inbox'],
  ['Process the inbox', '/inbox/process'],
  ['Weekly review', '/review'],
  ['All tasks', '/tasks'],
  ['Week', '/week'],
  ['Agenda', '/agenda'],
  ['Projects', '/projects'],
  ['Routines', '/routines'],
  ['Focus', '/focus'],
  ['Tracking', '/tracking'],
  ['Summary & journal', '/summary'],
  ['Trends', '/trends'],
  ['Goals', '/goals'],
  ['Rules', '/rules'],
  ['Namedays & birthdays', '/occasions'],
  ['Settings', '/settings'],
  ['Help', '/help'],
]

export function paletteItems(actions: Entry[]): Entry[] {
  const out: Entry[] = [...actions]
  for (const [title, href] of PAGES) out.push({ key: `page:${href}`, kind: 'page', title, href })
  out.push({ key: 'page:plan', kind: 'page', title: 'Plan today', href: `/plan/${store.today}` })
  out.push({ key: 'page:print', kind: 'page', title: 'Print today', href: `/print/${store.today}` })
  const tasks = [...store.tasks.values()].filter((t) => !t.deleted_at)
  // Open tasks first, then the most recently changed.
  tasks.sort((a, b) => Number(a.status !== 'open') - Number(b.status !== 'open') || b.updated_at.localeCompare(a.updated_at))
  for (const t of tasks) {
    const where = t.project_id ? store.projectPath(t.project_id) : 'Inbox'
    const status = t.status === 'open' ? '' : ` · ${t.status === 'done' ? 'done' : t.status.replace('_', ' ')}`
    out.push({
      key: `task:${t.id}`,
      kind: 'task',
      title: t.title,
      sub: `${where}${status}`,
      text: [t.notes, ...t.checklist.map((i) => i.text)].filter(Boolean).join(' · '),
      closed: t.status !== 'open',
      taskId: t.id,
    })
  }
  for (const p of store.projects.values()) {
    if (p.deleted_at) continue
    out.push({
      key: `project:${p.id}`,
      kind: 'project',
      title: p.name,
      sub: p.archived_at ? 'Archived project' : p.status === 'idea' ? 'Project idea' : 'Project',
      text: p.description,
      closed: !!p.archived_at,
      href: `/projects/${p.id}`,
    })
  }
  for (const s of store.series.values()) {
    if (s.deleted_at) continue
    const ended = !!s.until && s.until < store.today
    out.push({ key: `routine:${s.id}`, kind: 'routine', title: s.title, sub: ended ? 'Routine (ended)' : 'Routine', text: s.notes, closed: ended, run: () => (ui.routine = s.id) })
  }
  for (const r of store.dayRecords.values()) {
    const text = [r.journal, r.went_well, r.went_badly, r.tomorrow].filter((x) => x.trim()).join(' · ')
    if (!text) continue
    out.push({ key: `journal:${r.date}`, kind: 'journal', title: `Journal, ${longDate(r.date)}`, sub: 'Journal', text, href: `/summary/${r.date}` })
  }
  return out
}
