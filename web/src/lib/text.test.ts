import { describe, expect, it } from 'vitest'
import { linkify } from './text'

describe('linkify', () => {
  it('keeps plain text as one part', () => {
    expect(linkify('Small one by the shed')).toEqual([{ text: 'Small one by the shed' }])
    expect(linkify('')).toEqual([])
  })
  it('finds links and leaves sentence punctuation outside them', () => {
    expect(linkify('Kits: https://example.org/kits?a=1, or (https://x.ee/b).')).toEqual([
      { text: 'Kits: ' },
      { text: 'https://example.org/kits?a=1', url: 'https://example.org/kits?a=1' },
      { text: ', or (' },
      { text: 'https://x.ee/b', url: 'https://x.ee/b' },
      { text: ').' },
    ])
  })
  it('only links http(s)', () => {
    expect(linkify('javascript:alert(1) ftp://a.b')).toEqual([{ text: 'javascript:alert(1) ftp://a.b' }])
  })
})
