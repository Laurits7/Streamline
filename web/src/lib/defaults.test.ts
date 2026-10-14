import { describe, expect, it } from 'vitest'
import type { TaskTemplate } from './api/types/TaskTemplate'
import { allDefaults, matchDefaults, tasksFor } from './defaults'

const step = (text: string) => ({ id: text, text, done: false })
const tpl = (id: string, title: string, kinds: { id: string; name: string; steps?: string[] }[] = [], steps: string[] = []) =>
  ({
    id,
    title,
    checklist: steps.map(step),
    kinds: kinds.map((k) => ({ id: k.id, name: k.name, checklist: (k.steps ?? []).map(step) })),
  }) as unknown as TaskTemplate

const laundry = tpl('L', 'Laundry', [{ id: 'lin', name: 'Linen', steps: ['Wash at 60°', 'Iron'] }, { id: 'col', name: 'Colours' }], ['Wash', 'Fold'])
const garbage = tpl('G', 'Take out garbage', [{ id: 'bio', name: 'Bio' }, { id: 'pap', name: 'Paper' }])
const towels = tpl('T', 'Towels laundry')
const all = [towels, garbage, laundry]

describe('default task suggestions (D-86)', () => {
  it('lists every default task by title', () => {
    expect(allDefaults(all).map((m) => m.tpl.id)).toEqual(['L', 'G', 'T'])
  })
  it('matches titles, those starting with the text first, then kinds', () => {
    expect(matchDefaults(all, 'laun').map((m) => m.tpl.id)).toEqual(['L', 'T'])
    expect(matchDefaults(all, 'l')).toEqual([])
    expect(matchDefaults(all, 'pap').map((m) => [m.tpl.id, m.kind?.id])).toEqual([['G', 'pap']])
    // A kind isn't listed again when its default task already matched.
    expect(matchDefaults(all, 'garb').map((m) => m.kind)).toEqual([null])
  })
  it('makes one task per chosen kind, with its own steps or the default task’s', () => {
    expect(tasksFor(laundry, [])).toEqual([{ title: 'Laundry', checklist: [step('Wash'), step('Fold')] }])
    expect(tasksFor(laundry, ['col', 'lin']).map((t) => [t.title, t.checklist.map((i) => i.text)])).toEqual([
      ['Laundry: Linen', ['Wash at 60°', 'Iron']],
      ['Laundry: Colours', ['Wash', 'Fold']],
    ])
    expect(tasksFor(garbage, ['pap'])).toEqual([{ title: 'Take out garbage: Paper', checklist: [] }])
  })
})
