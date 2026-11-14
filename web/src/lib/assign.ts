// Who does a shared chore (D-116): its default assignee, or members taking turns.
// Mirrors `crates/domain/src/assign.rs`.

/**
 * The person for the `turn`th occurrence or run (from 0): with a rotation, the people in it
 * still in the group take turns in order; otherwise the default, while they're in the group.
 */
export function assigneeFor(fallback: string | null, rotation: string[], members: string[], turn: number): string | null {
  const turns = rotation.filter((u) => members.includes(u))
  if (turns.length) return turns[turn % turns.length]
  return fallback && members.includes(fallback) ? fallback : null
}

/** Whether a task belongs in my lists: it's for anyone, or for me. */
export function isForMe(assignee: string | null, me: string | null | undefined): boolean {
  return !assignee || assignee === me
}
