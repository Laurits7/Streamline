// Side-by-side lanes for overlapping items on a time grid (the day timeline and the week
// calendar): each cluster of overlapping items gets as many lanes as it needs.

export type Slot = { start: number; dur: number; lane: number; lanes: number }

/** Shortest height an item takes up (minutes), so zero-length items still get a lane. */
const MIN_DUR = 15

/** Set `lane` and `lanes` on every slot (in place) and return them. */
export function layoutLanes<T extends Slot>(slots: T[]): T[] {
  const sorted = [...slots].sort((a, b) => a.start - b.start || b.dur - a.dur)
  let cluster: T[] = []
  let clusterEnd = -1
  const close = () => {
    const n = Math.max(1, ...cluster.map((b) => b.lane + 1))
    cluster.forEach((b) => (b.lanes = n))
  }
  for (const b of sorted) {
    if (b.start >= clusterEnd && cluster.length) {
      close()
      cluster = []
    }
    const laneEnds: number[] = []
    for (const c of cluster) laneEnds[c.lane] = Math.max(laneEnds[c.lane] ?? 0, c.start + Math.max(c.dur, MIN_DUR))
    let lane = 0
    while (laneEnds[lane] !== undefined && laneEnds[lane] > b.start) lane++
    b.lane = lane
    cluster.push(b)
    clusterEnd = Math.max(clusterEnd, b.start + Math.max(b.dur, MIN_DUR))
  }
  if (cluster.length) close()
  return slots
}
