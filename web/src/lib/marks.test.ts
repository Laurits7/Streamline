import { describe, expect, it } from 'vitest'
import { pickSub, pickTop } from './marks'

// Same cases as streamline_domain::marks, so client and server agree.
describe('project marks', () => {
  it('hands out unused pairs, then the least used', () => {
    expect(pickTop([])).toEqual(['#dc2626', 'circle'])
    expect(pickTop([['#dc2626', 'circle']])).toEqual(['#1e66cc', 'square'])
    expect(pickTop([['#1e66cc', 'square']])).toEqual(['#dc2626', 'circle'])
    expect(pickTop([[null, 'circle']])).toEqual(['#dc2626', 'circle'])
  })
  it('gives subprojects a shape their parent and siblings do not have', () => {
    expect(pickSub('circle', [])).toBe('square')
    expect(pickSub('square', ['circle'])).toBe('triangle')
    expect(pickSub('circle', ['square', 'triangle', 'diamond', 'hexagon', 'star'])).toBe('square')
  })
})
