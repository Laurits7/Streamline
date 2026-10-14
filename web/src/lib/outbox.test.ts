import { describe, expect, it } from 'vitest'
import { syncNotice } from './outbox'

const titles: Record<string, string> = { A: 'Laundry', C: 'Old chore' }
const titleOf = (_k: string, id: string) => titles[id] ?? null

describe('synced-with-changes notice (D-79)', () => {
  it('is quiet when everything went through', () => {
    expect(syncNotice({ pending: 0, sent: 3, conflicts: [], dropped: [] }, titleOf)).toBeNull()
  })
  it('names clashes and refused changes', () => {
    const text = syncNotice(
      {
        pending: 0,
        sent: 2,
        conflicts: [
          { kind: 'task', id: 'A', field: 'title', item: null, kept: 'server' },
          { kind: 'task', id: 'X', field: 'start_time', item: null, kept: 'client' },
        ],
        dropped: [
          { kind: 'task', id: 'C', method: 'PATCH', status: 404, message: 'Not Found' },
          { kind: 'day_entry', id: 'E', method: 'PATCH', status: 400, message: 'bad time' },
        ],
      },
      titleOf,
    )
    expect(text).toBe(
      'Synced the changes made offline. “Laundry”: kept the newer title from elsewhere; a task: your time replaced an older change; ' +
        "“Old chore” was deleted elsewhere, so your change was dropped; a day entry: your change couldn't be saved (bad time).",
    )
  })
  it('keeps it short', () => {
    const conflicts = Array.from({ length: 6 }, (_, i) => ({ kind: 'task', id: `T${i}`, field: 'notes', item: null, kept: 'server' as const }))
    expect(syncNotice({ pending: 0, sent: 6, conflicts, dropped: [] }, titleOf)).toMatch(/; and 2 more\.$/)
  })
})
