// Default tasks in quick-add (D-71, D-86): matching what's typed against their titles and
// kinds, and the tasks a pick turns into (one per chosen kind).

import type { ChecklistItem } from './api/types/ChecklistItem'
import type { TaskTemplate } from './api/types/TaskTemplate'
import type { TemplateKind } from './api/types/TemplateKind'

export type DefaultMatch = { tpl: TaskTemplate; kind: TemplateKind | null }

const byTitle = (a: TaskTemplate, b: TaskTemplate) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' })
const wordStarts = (text: string, s: string) => text.toLowerCase().split(/\s+/).some((w) => w.startsWith(s))

/** Every default task, by title. */
export function allDefaults(templates: Iterable<TaskTemplate>): DefaultMatch[] {
  return [...templates].sort(byTitle).map((tpl) => ({ tpl, kind: null }))
}

/**
 * Default tasks whose title (or a word of it) starts with what's typed, those whose title
 * starts with it first; then kinds matching the same way ("lin…" → Laundry › Linen).
 */
export function matchDefaults(templates: Iterable<TaskTemplate>, q: string, limit = 6): DefaultMatch[] {
  const s = q.trim().toLowerCase()
  if (s.length < 2) return []
  const list = [...templates].sort(byTitle)
  const starts = (t: TaskTemplate) => t.title.toLowerCase().startsWith(s)
  const titles: DefaultMatch[] = list
    .filter((t) => wordStarts(t.title, s))
    .sort((a, b) => Number(starts(b)) - Number(starts(a)))
    .map((tpl) => ({ tpl, kind: null }))
  const kinds: DefaultMatch[] = list.flatMap((tpl) =>
    titles.some((m) => m.tpl.id === tpl.id) ? [] : tpl.kinds.filter((k) => wordStarts(k.name, s)).map((kind) => ({ tpl, kind })),
  )
  return [...titles, ...kinds].slice(0, limit)
}

/** Title and steps of the tasks a default task makes: one per chosen kind, or itself. */
export function tasksFor(tpl: TaskTemplate, kindIds: string[]): { title: string; checklist: ChecklistItem[] }[] {
  const chosen = tpl.kinds.filter((k) => kindIds.includes(k.id))
  if (!chosen.length) return [{ title: tpl.title, checklist: tpl.checklist }]
  return chosen.map((k) => ({ title: `${tpl.title}: ${k.name}`, checklist: k.checklist.length ? k.checklist : tpl.checklist }))
}
