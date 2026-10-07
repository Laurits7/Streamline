// The daily health check-in (D-75): one answer per day. Mirrors `streamline_domain::health`.

import type { HealthDay } from './api/types/HealthDay'

export type HealthStatus = 'great' | 'ok' | 'unwell' | 'sick' | 'injured'

export const STATUSES: { id: HealthStatus; label: string; emoji: string }[] = [
  { id: 'great', label: 'Feeling great', emoji: '💪' },
  { id: 'ok', label: 'OK', emoji: '🙂' },
  { id: 'unwell', label: 'Not feeling well', emoji: '😕' },
  { id: 'sick', label: 'Sick', emoji: '🤒' },
  { id: 'injured', label: 'Injured', emoji: '🩹' },
]

export const KINDS: Record<'sick' | 'injured', { id: string; label: string }[]> = {
  sick: [
    { id: 'cold', label: 'Cold' },
    { id: 'flu', label: 'Flu' },
    { id: 'fever', label: 'Fever' },
    { id: 'stomach', label: 'Stomach' },
    { id: 'headache', label: 'Headache' },
    { id: 'other', label: 'Other…' },
  ],
  injured: [
    { id: 'back', label: 'Back' },
    { id: 'neck', label: 'Neck' },
    { id: 'hand', label: 'Hand / finger' },
    { id: 'arm', label: 'Arm / shoulder' },
    { id: 'knee', label: 'Knee / leg' },
    { id: 'foot', label: 'Foot / ankle' },
    { id: 'other', label: 'Other…' },
  ],
}

export const hasKinds = (s: string): s is 'sick' | 'injured' => s === 'sick' || s === 'injured'

export function statusOf(s: string) {
  return STATUSES.find((x) => x.id === s)
}

/** "Sick · flu", "Injured · back: lifting boxes", "Feeling great". */
export function healthLabel(h: Pick<HealthDay, 'status' | 'kind' | 'note'>): string {
  const base = statusOf(h.status)?.label ?? h.status
  const kind = hasKinds(h.status) && h.kind ? KINDS[h.status].find((k) => k.id === h.kind) : null
  const what = kind && kind.id !== 'other' ? kind.label.toLowerCase() : ''
  const parts = [what, h.note].filter(Boolean)
  return parts.length ? `${base} · ${parts.join(': ')}` : base
}

/** A day that didn't go well health-wise (marked on the month calendar). */
export const isAiling = (s: string) => s === 'sick' || s === 'injured'
