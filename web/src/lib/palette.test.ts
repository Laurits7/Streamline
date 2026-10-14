import { describe, expect, it } from 'vitest'
import { fold, score, search, snippet, type PaletteItem } from './palette'

const task = (title: string, extra: Partial<PaletteItem> = {}): PaletteItem => ({ key: title, kind: 'task', title, ...extra })

describe('command palette search (10.2)', () => {
  it('ranks title start, then word start, then anywhere in the title, then notes and steps', () => {
    const items = [
      task('Fix the plumbing', { key: 'a' }),
      task('Plumber appointment', { key: 'b' }),
      task('Call someone', { key: 'c', text: 'ask about the plumber' }),
      task('Replumb the sink', { key: 'd' }),
    ]
    expect(search(items, 'plumb').map((i) => i.key)).toEqual(['b', 'a', 'd', 'c'])
  })

  it('needs every word, folds accents and case', () => {
    const items = [task('Käsk: osta piima', { key: 'et' }), task('Buy milk', { key: 'en', text: 'oat milk' })]
    expect(search(items, 'kask piima').map((i) => i.key)).toEqual(['et'])
    expect(search(items, 'MILK oat').map((i) => i.key)).toEqual(['en'])
    expect(search(items, 'milk bread')).toEqual([])
    expect(fold('Café Õun')).toBe('cafe oun')
  })

  it('finds checklist steps and allows letters in order', () => {
    expect(score(task('Laundry', { text: 'Sort · Wash · Fold' }), 'fold')).toBeGreaterThan(0)
    expect(score(task('Call plumber'), 'clplm')).toBeGreaterThan(0)
    expect(score(task('Call plumber'), 'zz')).toBe(0)
  })

  it('puts pages and actions slightly ahead, finished items behind', () => {
    const items: PaletteItem[] = [
      task('Today’s groceries', { key: 'task' }),
      { key: 'page', kind: 'page', title: 'Today' },
      task('Today done', { key: 'done', closed: true }),
    ]
    expect(search(items, 'today').map((i) => i.key)).toEqual(['page', 'task', 'done'])
  })

  it('keeps the given order without a query, and limits the list', () => {
    const items = Array.from({ length: 40 }, (_, i) => task(`T${i}`))
    expect(search(items, '  ').length).toBe(30)
    expect(search(items, '', 5).map((i) => i.title)).toEqual(['T0', 'T1', 'T2', 'T3', 'T4'])
  })

  it('shows a snippet around the match', () => {
    const text = 'Long day. In the evening we talked about the garden plans and the new greenhouse for spring.'
    expect(snippet(text, 'greenhouse', 30)).toBe('…the new greenhouse for…')
    expect(snippet('Short note', 'note')).toBe('Short note')
    expect(snippet('Nothing here', 'garden')).toBeNull()
  })
})
