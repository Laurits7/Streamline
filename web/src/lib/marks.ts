// Project marks (D-74): a colour and a shape per project, so projects can be told apart at
// a glance. Mirrors `streamline_domain::marks`, so a new project looks the same before and
// after the server answers.

export const PALETTE = ['#dc2626', '#1e66cc', '#16a34a', '#ea580c', '#7c3aed', '#0891b2', '#db2777', '#ca8a04', '#4f46e5']
export const SHAPES = ['circle', 'square', 'triangle', 'diamond', 'hexagon', 'star'] as const
export type Shape = (typeof SHAPES)[number]

/** All 54 pairs in hand-out order: each round uses every colour, shapes shift by one per round. */
function sequence(): [string, Shape][] {
  const out: [string, Shape][] = []
  for (let round = 0; round < SHAPES.length; round++)
    PALETTE.forEach((color, c) => out.push([color, SHAPES[(c + round) % SHAPES.length]]))
  return out
}

/** For a new top-level project: the first unused pair, or the least-used one. */
export function pickTop(used: [string | null, string][]): [string, Shape] {
  let best: [string, Shape] = [PALETTE[0], SHAPES[0]]
  let bestCount = Infinity
  for (const pair of sequence()) {
    const n = used.filter(([c, s]) => c === pair[0] && s === pair[1]).length
    if (n < bestCount) [best, bestCount] = [pair, n]
  }
  return best
}

/** For a new subproject (parent's colour): a shape its parent and siblings don't have, or the least-used one. */
export function pickSub(parent: string, siblings: string[]): Shape {
  let best: Shape = SHAPES[0]
  let bestKey = [Infinity, 1]
  for (const s of SHAPES) {
    const key = [siblings.filter((x) => x === s).length + (s === parent ? 1 : 0), s === parent ? 1 : 0]
    if (key[0] < bestKey[0] || (key[0] === bestKey[0] && key[1] < bestKey[1])) [best, bestKey] = [s, key]
  }
  return best
}
