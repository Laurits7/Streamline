import { describe, expect, it } from 'vitest'
import { placeCard, TOUR } from './tour'

const view = { width: 400, height: 800 }
const card = { width: 300, height: 200 }

describe('tour card placement', () => {
  it('centres the card when nothing is highlighted', () => {
    expect(placeCard(null, card, view)).toEqual({ top: 300, left: 50 })
  })

  it('puts the card below the target when it fits', () => {
    expect(placeCard({ top: 100, left: 20, width: 360, height: 50 }, card, view)).toEqual({ top: 162, left: 50 })
  })

  it('puts it above when there is no room below', () => {
    expect(placeCard({ top: 600, left: 20, width: 360, height: 100 }, card, view)).toEqual({ top: 388, left: 50 })
  })

  it('falls back to the bottom of the screen for a tall target', () => {
    expect(placeCard({ top: 50, left: 0, width: 400, height: 700 }, card, view)).toEqual({ top: 588, left: 50 })
  })

  it('keeps the card on screen next to an edge', () => {
    expect(placeCard({ top: 10, left: 360, width: 30, height: 30 }, card, view).left).toBe(88)
    expect(placeCard({ top: 10, left: 0, width: 20, height: 30 }, card, view).left).toBe(12)
  })

  it('has a title and text for every step, starting and ending on Today', () => {
    for (const s of TOUR) expect(s.title && s.body && s.route.startsWith('/')).toBeTruthy()
    expect(TOUR[0].route).toBe('/')
    expect(TOUR.at(-1)!.route).toBe('/')
  })
})
