import { describe, expect, it } from 'vitest'
import type { Task } from './api/types/Task'
import { current, decide, decisionFor, DECISIONS, start, tally, triageQueue, undo } from './triage'

const t = (id: string, extra: Partial<Task> = {}) =>
  ({ id, status: 'open', project_id: null, deleted_at: null, waiting_since: null, series_id: null, created_at: `2026-10-0${id}T10:00:00Z`, ...extra }) as Task

describe('inbox triage (10.3)', () => {
  it('queues what still needs a decision, oldest first', () => {
    const tasks = [
      t('3'),
      t('1'),
      t('2', { project_id: 'P' }),
      t('4', { status: 'done' }),
      t('5', { waiting_since: '2026-10-05T00:00:00Z' }),
      t('6', { series_id: 'S' }),
      t('7'),
    ]
    expect(triageQueue(tasks, (id) => id === '7').map((x) => x.id)).toEqual(['1', '3'])
  })

  it('moves through the queue, undoes step by step and counts decisions', () => {
    let s = start(['a', 'b', 'c'])
    expect(current(s)).toBe('a')
    s = decide(s, 'today')
    s = decide(s, 'skip')
    expect(current(s)).toBe('c')
    const u = undo(s)
    expect(u.step).toEqual({ id: 'b', decision: 'skip' })
    s = decide(u.session, 'drop')
    s = decide(s, 'project')
    expect(current(s)).toBeNull()
    expect(decide(s, 'done')).toBe(s)
    expect(tally(s)).toEqual({ today: 1, drop: 1, project: 1 })
    expect(undo(start(['a'])).step).toBeNull()
  })

  it('maps keys to decisions', () => {
    expect(decisionFor('t')).toBe('today')
    expect(decisionFor('m')).toBe('tomorrow')
    expect(decisionFor('Backspace')).toBe('drop')
    expect(decisionFor('Delete')).toBe('drop')
    expect(decisionFor('ArrowRight')).toBe('skip')
    expect(decisionFor('u')).toBe('undo')
    expect(decisionFor('q')).toBeNull()
    expect(new Set(DECISIONS.map((d) => d.key)).size).toBe(DECISIONS.length)
  })
})
