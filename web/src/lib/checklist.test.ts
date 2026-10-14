import { describe, expect, it } from 'vitest'
import { statusAfter } from './checklist'

const it_ = (id: string, done: boolean) => ({ id, text: id, done })

describe('checklist status (D-80)', () => {
  it('completes on the last tick and reopens on an untick', () => {
    const half = [it_('a', true), it_('b', false)]
    const all = [it_('a', true), it_('b', true)]
    expect(statusAfter(half, all, 'open')).toBe('done')
    expect(statusAfter(half, all.slice(0, 1), 'open')).toBe('done')
    expect(statusAfter(all, half, 'done')).toBe('open')
    expect(statusAfter(all, all, 'open')).toBeNull()
    expect(statusAfter([], [], 'open')).toBeNull()
    expect(statusAfter(half, all, 'skipped')).toBeNull()
    expect(statusAfter(all, [...all, it_('c', false)], 'done')).toBeNull()
  })
})
