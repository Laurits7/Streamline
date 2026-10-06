// GPS matching for places (D-45): which saved place, if any, a position is at.
import type { Place } from './api/types/Place'

/** Great-circle distance in metres. */
export function distanceM(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const R = 6_371_000
  const rad = (d: number) => (d * Math.PI) / 180
  const dLat = rad(lat2 - lat1)
  const dLon = rad(lon2 - lon1)
  const a = Math.sin(dLat / 2) ** 2 + Math.cos(rad(lat1)) * Math.cos(rad(lat2)) * Math.sin(dLon / 2) ** 2
  return 2 * R * Math.asin(Math.min(1, Math.sqrt(a)))
}

/**
 * The place whose radius contains the position (the closest one if several overlap),
 * allowing for the position's own accuracy. `null` = not at any saved place.
 */
export function placeAt(places: Place[], lat: number, lon: number, accuracyM = 0): Place | null {
  let best: Place | null = null
  let bestD = Infinity
  for (const p of places) {
    if (p.lat === null || p.lon === null) continue
    const d = distanceM(lat, lon, p.lat, p.lon)
    if (d <= p.radius_m + Math.min(accuracyM, 500) && d < bestD) {
      best = p
      bestD = d
    }
  }
  return best
}
