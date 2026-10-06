// Double-booking (SPEC §6.11). Mirror of crates/domain/src/conflicts.rs so the day view
// can mark overlaps instantly; times are minutes from local midnight, intervals half-open.

export type ItemKind = 'task' | 'event' | 'block'
export type Item = { id: string; kind: ItemKind; start: number; end: number }
export type Conflict = { a: string; aKind: ItemKind; b: string; bKind: ItemKind; minutes: number }

const clash = (a: ItemKind, b: ItemKind) =>
  !((a === 'task' && b === 'block') || (a === 'block' && b === 'task') || (a === 'event' && b === 'event'))

/** Every clashing pair that overlaps, earliest first. Zero-length items never conflict. */
export function findConflicts(items: Item[]): Conflict[] {
  const sorted = items
    .filter((i) => i.end > i.start)
    .sort((x, y) => x.start - y.start || x.end - y.end || (x.id < y.id ? -1 : x.id > y.id ? 1 : 0))
  const out: Conflict[] = []
  for (let i = 0; i < sorted.length; i++) {
    const a = sorted[i]
    for (const b of sorted.slice(i + 1)) {
      if (b.start >= a.end) break
      if (clash(a.kind, b.kind))
        out.push({ a: a.id, aKind: a.kind, b: b.id, bKind: b.kind, minutes: Math.min(a.end, b.end) - b.start })
    }
  }
  return out
}

/** Earliest start ≥ `from` where `minutes` fit in `window` without touching busy time. */
export function nextFreeSlot(busy: [number, number][], minutes: number, from: number, window: [number, number]) {
  const lo = Math.max(from, window[0])
  const candidates = [lo, ...busy.map((b) => b[1]).filter((e) => e >= lo)].sort((a, b) => a - b)
  return (
    candidates.find((s) => s + minutes <= window[1] && busy.every(([bs, be]) => be <= bs || s + minutes <= bs || s >= be)) ??
    null
  )
}
