import { describe, expect, it } from 'vitest'
import { createsCycle, isBlocked } from './deps'

describe('prerequisites (mirror of domain::deps)', () => {
  const g: Record<string, string[]> = { fold: ['dry'], dry: ['wash'], wash: [] }
  const depsOf = (id: string) => g[id] ?? []
  it('detects cycles like the server', () => {
    expect(createsCycle('wash', ['fold'], depsOf)).toBe(true)
    expect(createsCycle('wash', ['wash'], depsOf)).toBe(true)
    expect(createsCycle('fold', ['wash', 'dry'], depsOf)).toBe(false)
    expect(createsCycle('iron', ['dry'], depsOf)).toBe(false)
  })
  it('blocks only on open prerequisites', () => {
    const st: Record<string, 'open' | 'done' | 'wont_do'> = { a: 'done', b: 'open', c: 'wont_do' }
    expect(isBlocked(['a', 'b'], (id) => st[id])).toBe(true)
    expect(isBlocked(['a', 'c', 'gone'], (id) => st[id as 'a'])).toBe(false)
  })
})
