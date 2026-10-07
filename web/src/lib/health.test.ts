import { describe, expect, it } from 'vitest'
import { healthLabel, isAiling, KINDS, STATUSES } from './health'

describe('health check-in', () => {
  it('labels a day', () => {
    expect(healthLabel({ status: 'great', kind: null, note: '' })).toBe('Feeling great')
    expect(healthLabel({ status: 'sick', kind: 'flu', note: '' })).toBe('Sick · flu')
    expect(healthLabel({ status: 'injured', kind: 'back', note: 'lifting boxes' })).toBe('Injured · back: lifting boxes')
    expect(healthLabel({ status: 'sick', kind: 'other', note: 'migraine' })).toBe('Sick · migraine')
  })
  it('matches the server lists', () => {
    expect(STATUSES.map((s) => s.id)).toEqual(['great', 'ok', 'unwell', 'sick', 'injured'])
    expect(KINDS.sick.map((k) => k.id)).toEqual(['cold', 'flu', 'fever', 'stomach', 'headache', 'other'])
    expect(KINDS.injured.map((k) => k.id)).toEqual(['back', 'neck', 'hand', 'arm', 'knee', 'foot', 'other'])
    expect(isAiling('sick') && isAiling('injured') && !isAiling('unwell')).toBe(true)
  })
})
