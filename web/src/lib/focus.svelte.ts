// Client side of the focus timer: one shared clock for all countdown displays, and
// formatting. The timer itself lives on the server (store.focusTimer).
import { store } from './store.svelte'

/** Server-corrected "now", updated every second while a timer runs (see FocusTicker). */
export const clock = $state({ now: Date.now() })

export const PHASE_LABEL: Record<string, string> = {
  idle: 'Not focusing',
  work: 'Focus',
  short_break: 'Short break',
  long_break: 'Long break',
}

export function mmss(ms: number): string {
  const s = Math.ceil(ms / 1000)
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
}

/** Work interval waiting to be started (after a break, or after skipping one). */
export const isWaiting = () => {
  const t = store.focusTimer
  return t.phase === 'work' && t.running_since_ms === null && t.elapsed_ms === 0
}

/** A short chime and a buzz when an interval ends. Silent if audio isn't allowed yet. */
export function cue() {
  if (store.pref('focus_sound', true)) {
    try {
      const ctx = new AudioContext()
      ;[660, 880, 990].forEach((f, i) => {
        const o = ctx.createOscillator()
        const g = ctx.createGain()
        const t = ctx.currentTime + i * 0.22
        o.frequency.value = f
        g.gain.setValueAtTime(0.0001, t)
        g.gain.exponentialRampToValueAtTime(0.2, t + 0.02)
        g.gain.exponentialRampToValueAtTime(0.0001, t + 0.35)
        o.connect(g).connect(ctx.destination)
        o.start(t)
        o.stop(t + 0.4)
      })
      setTimeout(() => ctx.close(), 1500)
    } catch {
      /* no audio */
    }
  }
  navigator.vibrate?.([200, 100, 200])
}
