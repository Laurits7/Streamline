import { describe, expect, it } from 'vitest'
import { assigneeFor, isForMe } from './assign'

describe('assignment (D-116)', () => {
  const members = ['anna', 'ben', 'cid']
  it('uses the default while they are in the group', () => {
    expect(assigneeFor('ben', [], members, 3)).toBe('ben')
    expect(assigneeFor('zed', [], members, 0)).toBeNull()
    expect(assigneeFor(null, [], members, 0)).toBeNull()
  })
  it('takes turns in order, skipping people who left', () => {
    expect([0, 1, 2, 3].map((n) => assigneeFor(null, ['ben', 'anna'], members, n))).toEqual(['ben', 'anna', 'ben', 'anna'])
    expect([0, 1, 2].map((n) => assigneeFor(null, ['anna', 'zed', 'cid'], members, n))).toEqual(['anna', 'cid', 'anna'])
    expect(assigneeFor('anna', ['zed'], members, 1)).toBe('anna')
  })
  it('keeps unassigned and own tasks in my lists only', () => {
    expect(isForMe(null, 'anna')).toBe(true)
    expect(isForMe('anna', 'anna')).toBe(true)
    expect(isForMe('ben', 'anna')).toBe(false)
  })
})
