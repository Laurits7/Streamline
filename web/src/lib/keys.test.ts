import { describe, expect, it } from 'vitest'
import { GO, keyReader, SHORTCUTS, typing } from './keys'

describe('keyboard shortcuts (10.2)', () => {
  it('reads single keys', () => {
    const read = keyReader()
    expect(read('n', 0)).toEqual({ kind: 'new' })
    expect(read('j', 0)).toEqual({ kind: 'move', by: 1 })
    expect(read('k', 0)).toEqual({ kind: 'move', by: -1 })
    expect(read('x', 0)).toEqual({ kind: 'complete' })
    expect(read('/', 0)).toEqual({ kind: 'palette' })
    expect(read('?', 0)).toEqual({ kind: 'help' })
    expect(read('q', 0)).toBeNull()
  })

  it('goes to a page with g and a letter, within a moment', () => {
    const read = keyReader()
    expect(read('g', 1000)).toBeNull()
    expect(read('i', 1500)).toEqual({ kind: 'go', to: '/inbox' })
    // "t" right after a finished sequence is "plan today" again, not "go".
    expect(read('t', 1600)).toEqual({ kind: 'today' })
    // Too slow: the second key counts on its own.
    read('g', 5000)
    expect(read('t', 7000)).toEqual({ kind: 'today' })
    // g g goes to Goals; an unknown second key does nothing.
    read('g', 8000)
    expect(read('g', 8100)).toEqual({ kind: 'go', to: '/goals' })
    read('g', 9000)
    expect(read('z', 9100)).toBeNull()
    expect(read('n', 9200)).toEqual({ kind: 'new' })
  })

  it('stays out of the way while typing', () => {
    const el = (tagName: string, isContentEditable = false) => ({ tagName, isContentEditable }) as unknown as EventTarget
    expect(typing(el('INPUT'))).toBe(true)
    expect(typing(el('TEXTAREA'))).toBe(true)
    expect(typing(el('DIV', true))).toBe(true)
    expect(typing(el('BUTTON'))).toBe(false)
    expect(typing(null)).toBe(false)
  })

  it('lists every page shortcut in the help', () => {
    for (const k of Object.keys(GO)) expect(SHORTCUTS.some(([keys]) => keys === `g ${k}`)).toBe(true)
  })
})
