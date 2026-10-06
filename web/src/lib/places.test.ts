import { describe, expect, it } from 'vitest'
import type { Place } from './api/types/Place'
import { distanceM, placeAt } from './places'

const place = (id: string, lat: number | null, lon: number | null, radius_m = 200): Place => ({
  id, owner_user_id: 'U1', owner_group_id: null, name: id, lat, lon, radius_m, position: 'V',
  created_at: '', updated_at: '', deleted_at: null, rev: 1,
})

describe('places by GPS', () => {
  it('measures distances', () => {
    // One degree of latitude = 2πR/360 = 111 195 m for R = 6 371 km.
    expect(Math.round(distanceM(58, 26, 59, 26))).toBe(111195)
    // Tallinn -> Tartu town hall squares, roughly 160-165 km apart.
    const km = distanceM(59.4372, 24.7453, 58.3801, 26.7224) / 1000
    expect(km > 160 && km < 165).toBe(true)
    expect(distanceM(58.38, 26.72, 58.38, 26.72)).toBe(0)
  })

  it('finds the place within its radius, closest first, ignoring places without coordinates', () => {
    const home = place('home', 58.38, 26.72, 150)
    const shop = place('shop', 58.3809, 26.72, 150) // ~100 m north of home
    const town = place('town', null, null)
    expect(placeAt([home, shop, town], 58.38, 26.72)?.id).toBe('home')
    expect(placeAt([home, shop, town], 58.3808, 26.72)?.id).toBe('shop')
    expect(placeAt([home, shop, town], 58.4, 26.72)).toBeNull()
    // A fuzzy fix (accuracy 300 m) still counts as being at the nearest place in reach.
    expect(placeAt([home], 58.3830, 26.72, 300)?.id).toBe('home')
  })
})
