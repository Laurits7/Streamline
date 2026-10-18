import { describe, expect, it } from 'vitest'
import { goalTree, parentOptions, periodStart, reviewDue, subtreeIds } from './goals'

describe('goal reviews (mirror of domain::goals)', () => {
  it('periods', () => {
    expect(periodStart('weekly', '2026-10-07', 1)).toBe('2026-10-05')
    expect(periodStart('weekly', '2026-10-07', 7)).toBe('2026-10-04')
    expect(periodStart('monthly', '2026-10-07', 1)).toBe('2026-10-01')
    expect(periodStart('off', '2026-10-07', 1)).toBeNull()
  })
  it('due once per period', () => {
    expect(reviewDue('weekly', null, '2026-10-07', 1)).toBe(true)
    expect(reviewDue('weekly', '2026-10-04', '2026-10-07', 1)).toBe(true)
    expect(reviewDue('weekly', '2026-10-05', '2026-10-07', 1)).toBe(false)
    expect(reviewDue('monthly', '2026-10-31', '2026-11-01', 1)).toBe(true)
    expect(reviewDue('off', null, '2026-10-07', 1)).toBe(false)
  })
})

describe('goal tree (D-112)', () => {
  const g = (id: string, parent_id: string | null, position: string, kind: 'outcome' | 'habit' | 'rule' = 'outcome') => ({ id, parent_id, position, kind })
  // life > money, health > junk (rule); work; an orphan whose parent is gone
  const goals = [
    g('health', 'life', 'b'),
    g('work', null, 'b'),
    g('life', null, 'a'),
    g('junk', 'health', 'a', 'rule'),
    g('money', 'life', 'a'),
    g('orphan', 'gone', 'c'),
  ]

  it('orders depth first, siblings by position, orphans at the top', () => {
    expect(goalTree(goals).map((x) => `${'-'.repeat(x.depth)}${x.goal.id}:${x.children}`)).toEqual([
      'life:2',
      '-money:0',
      '-health:1',
      '--junk:0',
      'work:0',
      'orphan:0',
    ])
  })

  it('survives cycles in stored data', () => {
    const t = goalTree([g('a', 'b', 'a'), g('b', 'a', 'b'), g('c', null, 'c')])
    expect(t.map((x) => x.goal.id)).toEqual(['c'])
  })

  it('subtrees and where a goal can move', () => {
    expect([...subtreeIds(goals, 'life')].sort()).toEqual(['health', 'junk', 'life', 'money'])
    expect(parentOptions(goals, 'health').map((x) => x.id)).toEqual(['work', 'life', 'money', 'orphan'])
    expect(parentOptions(goals, null).map((x) => x.id)).not.toContain('junk')
  })
})
