import { describe, expect, it } from 'vitest'
import { placePopover, statusAfter } from './checklist'

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

describe('checklist popover placement (D-81)', () => {
  const pop = { width: 240, height: 200 }
  const view = { width: 1200, height: 800 }

  it('sits to the right of the item, aligned with its top', () => {
    expect(placePopover({ top: 100, left: 100, width: 300, height: 40 }, pop, view)).toEqual({ top: 100, left: 406 })
  })
  it('flips to the left near the right edge', () => {
    expect(placePopover({ top: 100, left: 800, width: 300, height: 40 }, pop, view)).toEqual({ top: 100, left: 554 })
  })
  it('stays on screen near the bottom and top', () => {
    expect(placePopover({ top: 760, left: 100, width: 300, height: 30 }, pop, view).top).toBe(592)
    expect(placePopover({ top: -50, left: 100, width: 300, height: 60 }, pop, view).top).toBe(8)
  })
  it('goes below or above a full-width item on a phone', () => {
    const phone = { width: 390, height: 800 }
    expect(placePopover({ top: 100, left: 60, width: 320, height: 40 }, pop, phone)).toEqual({ top: 146, left: 60 })
    expect(placePopover({ top: 700, left: 60, width: 320, height: 40 }, pop, phone)).toEqual({ top: 494, left: 60 })
    expect(placePopover({ top: 150, left: 200, width: 180, height: 600 }, pop, phone)).toEqual({ top: 592, left: 142 })
  })
  it('never starts off screen when the popover is wider than the screen', () => {
    expect(placePopover({ top: 100, left: 0, width: 200, height: 40 }, { width: 400, height: 100 }, { width: 320, height: 600 })).toEqual({ top: 146, left: 8 })
  })
})
