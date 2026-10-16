// The command palette's search (10.2): pages, actions, tasks (title, notes, checklist
// steps), projects, routines and journal days, ranked by where the words match.

export type PaletteKind = 'action' | 'page' | 'task' | 'project' | 'routine' | 'journal'

export type PaletteItem = {
  /** Unique within the list. */
  key: string
  kind: PaletteKind
  title: string
  /** Shown under the title (project path, date…). */
  sub?: string
  /** Searched as well, with less weight (notes, checklist steps, journal text). */
  text?: string
  /** Finished or archived: listed after open ones. */
  closed?: boolean
}

/** Lower case without accents, so "kask" finds "käsk" and "cafe" finds "café". */
export function fold(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase()
}

const wordStart = (hay: string, w: string) => hay.startsWith(w) || hay.includes(` ${w}`) || hay.includes(`-${w}`) || hay.includes(`(${w}`)

/** Letters of `w` in order somewhere in `hay` ("plmb" in "plumber"). */
function subsequence(hay: string, w: string): boolean {
  let i = 0
  for (const c of hay) if (c === w[i] && ++i === w.length) return true
  return w.length === 0
}

function wordScore(title: string, text: string, w: string): number {
  if (title.startsWith(w)) return 100
  if (wordStart(title, w)) return 70
  if (title.includes(w)) return 40
  if (wordStart(text, w)) return 20
  if (text.includes(w)) return 10
  if (w.length >= 3 && subsequence(title, w)) return 5
  return 0
}

/** How well `item` matches the query (0 = not at all). Every word has to match somewhere. */
export function score(item: PaletteItem, query: string): number {
  const words = fold(query).split(/\s+/).filter(Boolean)
  if (!words.length) return 1
  const title = fold(item.title)
  const text = fold(item.text ?? '')
  let total = 0
  for (const w of words) {
    const s = wordScore(title, text, w)
    if (!s) return 0
    total += s
  }
  if (item.kind === 'action' || item.kind === 'page') total += 10
  if (item.closed) total -= 30
  return Math.max(total, 1)
}

/** The best matches, best first; with no query, the list as given. */
export function search<T extends PaletteItem>(items: T[], query: string, limit = 30): T[] {
  if (!query.trim()) return items.slice(0, limit)
  return items
    .map((item, i) => ({ item, i, s: score(item, query) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s || a.item.title.length - b.item.title.length || a.i - b.i)
    .slice(0, limit)
    .map((x) => x.item)
}

/** A short piece of `text` around the first query word found in it, for showing why it matched. */
export function snippet(text: string, query: string, width = 70): string | null {
  const hay = fold(text)
  for (const w of fold(query).split(/\s+/).filter(Boolean)) {
    const at = hay.indexOf(w)
    if (at < 0) continue
    let start = Math.max(0, at - Math.floor((width - w.length) / 2))
    let end = Math.min(text.length, start + width)
    // Whole words only at the cut ends.
    if (start > 0) start = Math.min(at, text.indexOf(' ', start) + 1 || at)
    if (end < text.length) end = Math.max(at + w.length, text.lastIndexOf(' ', end))
    return `${start > 0 ? '…' : ''}${text.slice(start, end).replace(/\s+/g, ' ').trim()}${end < text.length ? '…' : ''}`
  }
  return null
}
