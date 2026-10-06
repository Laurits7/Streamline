import { describe, expect, it } from 'vitest'
import { findConflicts, nextFreeSlot, type Item } from './conflicts'

const it_ = (id: string, kind: Item['kind'], start: number, end: number): Item => ({ id, kind, start, end })

describe('conflicts (mirror of domain::conflicts)', () => {
  it('touching is fine, overlapping is not', () => {
    const c = findConflicts([
      it_('standup', 'event', 540, 570),
      it_('write', 'task', 570, 630),
      it_('call', 'task', 600, 660),
      it_('lunch', 'event', 650, 700),
    ])
    expect(c.map((x) => [x.a, x.b, x.minutes])).toEqual([
      ['write', 'call', 30],
      ['call', 'lunch', 10],
    ])
  })
  it('what may overlap', () => {
    const c = findConflicts([
      it_('deep', 'block', 480, 720),
      it_('task', 'task', 500, 560),
      it_('meet', 'event', 600, 660),
      it_('meet2', 'event', 630, 690),
      it_('admin', 'block', 700, 780),
      it_('point', 'event', 510, 510),
    ])
    expect(c.map((x) => [x.a, x.b, x.minutes])).toEqual([
      ['deep', 'meet', 60],
      ['deep', 'meet2', 60],
      ['deep', 'admin', 20],
    ])
  })
  it('free slots', () => {
    const busy: [number, number][] = [
      [540, 570],
      [600, 660],
    ]
    expect(nextFreeSlot(busy, 30, 540, [480, 1080])).toBe(570)
    expect(nextFreeSlot(busy, 45, 540, [480, 1080])).toBe(660)
    expect(nextFreeSlot(busy, 30, 400, [480, 1080])).toBe(480)
    expect(nextFreeSlot(busy, 60, 1030, [480, 1080])).toBeNull()
  })
})
