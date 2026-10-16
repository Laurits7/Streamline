import { describe, expect, it } from 'vitest'
import { layoutLanes } from './lanes'

const s = (start: number, dur: number) => ({ start, dur, lane: 0, lanes: 1 })
const at = (xs: { lane: number; lanes: number }[]) => xs.map((x) => [x.lane, x.lanes])

describe('time grid lanes', () => {
  it('leaves separate items full width', () => {
    expect(at(layoutLanes([s(540, 60), s(600, 30)]))).toEqual([[0, 1], [0, 1]])
  })
  it('puts overlapping items side by side, per cluster', () => {
    // 9:00–10:00 and 9:30–10:30 overlap; 11:00 stands alone.
    expect(at(layoutLanes([s(540, 60), s(570, 60), s(660, 30)]))).toEqual([[0, 2], [1, 2], [0, 1]])
  })
  it('reuses a lane that has become free within a cluster', () => {
    // A long item with two short ones after each other beside it.
    expect(at(layoutLanes([s(540, 120), s(540, 30), s(600, 30)]))).toEqual([[0, 2], [1, 2], [1, 2]])
  })
  it('gives zero-length items room', () => {
    expect(at(layoutLanes([s(540, 0), s(545, 10)]))).toEqual([[0, 2], [1, 2]])
  })
})
