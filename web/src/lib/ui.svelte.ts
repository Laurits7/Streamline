// Global UI state: which sheets are open.
export const ui = $state({
  /** Task id being edited in the task sheet. */
  editing: null as string | null,
  /** Date for which the "pull in tasks" sheet is open. */
  pullFor: null as string | null,
})

// Collapsed projects in project trees (remembered in this browser only).
const KEY = 'sl.collapsed'
function load(): string[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? '[]')
  } catch {
    return []
  }
}
export const collapsed = $state<{ ids: string[] }>({ ids: load() })
export function toggleCollapsed(id: string) {
  collapsed.ids = collapsed.ids.includes(id) ? collapsed.ids.filter((x) => x !== id) : [...collapsed.ids, id]
  try {
    localStorage.setItem(KEY, JSON.stringify(collapsed.ids))
  } catch {
    /* storage unavailable */
  }
}
export const isCollapsed = (id: string) => collapsed.ids.includes(id)
