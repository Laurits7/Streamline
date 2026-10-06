import { describe, expect, it } from 'vitest'
import { matrixPatch, quadrantOf, statusColumn } from './views'

describe('eisenhower matrix (D-22: scores 0-3, threshold 2)', () => {
  it('places tasks by their scores', () => {
    expect(quadrantOf({ importance: 3, urgency: 2 })).toBe('do')
    expect(quadrantOf({ importance: 2, urgency: 1 })).toBe('schedule')
    expect(quadrantOf({ importance: null, urgency: 3 })).toBe('delegate')
    expect(quadrantOf({ importance: 1, urgency: null })).toBe('eliminate')
  })

  it('only changes a score that crosses the threshold', () => {
    // Very important (3), not urgent -> Do first: importance stays 3, urgency rises to 2.
    expect(matrixPatch({ importance: 3, urgency: null }, 'do')).toEqual({ urgency: 2 })
    // Do first -> Delegate: importance cleared, urgency (3) untouched.
    expect(matrixPatch({ importance: 2, urgency: 3 }, 'delegate')).toEqual({ importance: null })
    // Same quadrant: nothing to do.
    expect(matrixPatch({ importance: 1, urgency: 1 }, 'eliminate')).toEqual({})
    // Low scores (1) are below the threshold and get raised.
    expect(matrixPatch({ importance: 1, urgency: 1 }, 'do')).toEqual({ importance: 2, urgency: 2 })
  })

  it('every move lands in the requested quadrant', () => {
    const scores = [null, 0, 1, 2, 3]
    for (const importance of scores)
      for (const urgency of scores)
        for (const q of ['do', 'schedule', 'delegate', 'eliminate'] as const) {
          const t = { importance, urgency }
          expect(quadrantOf({ ...t, ...matrixPatch(t, q) })).toBe(q)
        }
  })
})

describe('board status columns', () => {
  it('maps open/started/done', () => {
    expect(statusColumn({ status: 'open', started_at: null })).toBe('todo')
    expect(statusColumn({ status: 'open', started_at: '2026-10-06T10:00:00Z' })).toBe('doing')
    expect(statusColumn({ status: 'done', started_at: null })).toBe('done')
    expect(statusColumn({ status: 'wont_do', started_at: null })).toBeNull()
  })
})
