// Global UI state: which sheets are open.
export const ui = $state({
  /** Task id being edited in the task sheet. */
  editing: null as string | null,
  /** Date for which the "pull in tasks" sheet is open. */
  pullFor: null as string | null,
})
