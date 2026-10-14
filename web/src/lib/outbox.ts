// Changes made offline in the desktop app (9.5, D-79): how many are waiting, and what to
// tell the user once they've reached the server.

export type OutboxConflict = { kind: string; id: string; field: string; item: string | null; kept: 'server' | 'client' }
export type OutboxDropped = { kind: string; id: string; method: string; status: number; message: string | null }
export type OutboxReport = { pending: number; sent: number; conflicts: OutboxConflict[]; dropped: OutboxDropped[] }

const FIELD: Record<string, string> = {
  title: 'title',
  name: 'name',
  notes: 'notes',
  description: 'description',
  checklist: 'checklist step',
  start_time: 'time',
  duration_min: 'duration',
  estimate_min: 'estimate',
  due_date: 'due date',
  date: 'date',
  journal: 'journal',
  status: 'status',
}

/**
 * The "synced with changes" notice: null when everything went through as made (then a
 * short "synced" note will do), else one line per clash or refused change, at most four.
 */
export function syncNotice(r: OutboxReport, titleOf: (kind: string, id: string) => string | null): string | null {
  if (!r.conflicts.length && !r.dropped.length) return null
  const name = (kind: string, id: string) => {
    const t = titleOf(kind, id)
    return t ? `“${t}”` : `a ${kind.replace('_', ' ')}`
  }
  const lines = [
    ...r.conflicts.map((c) => {
      const field = FIELD[c.field] ?? c.field.replace('_', ' ')
      return c.kept === 'server'
        ? `${name(c.kind, c.id)}: kept the newer ${field} from elsewhere`
        : `${name(c.kind, c.id)}: your ${field} replaced an older change`
    }),
    ...r.dropped.map((d) =>
      d.status === 404 || d.status === 410
        ? `${name(d.kind, d.id)} was deleted elsewhere, so your change was dropped`
        : `${name(d.kind, d.id)}: your change couldn't be saved${d.message ? ` (${d.message})` : ''}`,
    ),
  ]
  const shown = lines.slice(0, 4)
  if (lines.length > 4) shown.push(`and ${lines.length - 4} more`)
  return `Synced the changes made offline. ${shown.join('; ')}.`
}
