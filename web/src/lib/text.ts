// Plain text with clickable web links (project descriptions), without rendering HTML.

export type TextPart = { text: string; url?: string }

// http(s) links; trailing punctuation that usually ends a sentence isn't part of them.
const URL_RE = /https?:\/\/[^\s<>"]*[^\s<>".,;:!?)\]'”’]/g

export function linkify(text: string): TextPart[] {
  const out: TextPart[] = []
  let last = 0
  for (const m of text.matchAll(URL_RE)) {
    if (m.index > last) out.push({ text: text.slice(last, m.index) })
    out.push({ text: m[0], url: m[0] })
    last = m.index + m[0].length
  }
  if (last < text.length) out.push({ text: text.slice(last) })
  return out
}
