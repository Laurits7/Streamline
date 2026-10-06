// Prerequisite helpers for instant UI feedback. Mirror of crates/domain/src/deps.rs;
// the server enforces the same rules.
import type { Task } from './api/types/Task'

/** Would giving `task` the prerequisites `deps` make tasks wait for each other in a circle? */
export function createsCycle(task: string, deps: string[], depsOf: (id: string) => string[]): boolean {
  const stack = [...deps]
  const seen = new Set<string>()
  while (stack.length) {
    const t = stack.pop()!
    if (t === task) return true
    if (seen.has(t)) continue
    seen.add(t)
    stack.push(...depsOf(t))
  }
  return false
}

/** Blocked while any prerequisite is still open (skipped/won't do/missed/deleted ones don't count, D-6). */
export function isBlocked(deps: string[], status: (id: string) => Task['status'] | undefined): boolean {
  return deps.some((id) => status(id) === 'open')
}
