<script lang="ts">
  // Drives the shared focus clock, signals the end of an interval (sound, vibration,
  // toast) and asks the server to move on. Also shows the countdown in the tab title.
  import { clock, cue, mmss, PHASE_LABEL } from '../focus.svelte'
  import { store } from '../store.svelte'
  import { toast } from '../toast.svelte'

  const running = $derived(store.focusTimer.running_since_ms !== null && store.focusTimer.phase !== 'idle')
  let fired = ''

  $effect(() => {
    if (!running) return
    const tick = () => {
      clock.now = store.serverNow()
      const t = store.focusTimer
      const key = `${t.phase}:${t.running_since_ms}:${t.elapsed_ms}`
      if (t.running_since_ms !== null && store.focusRemaining(clock.now) <= 0 && fired !== key) {
        fired = key
        cue()
        toast(t.phase === 'work' ? 'Time for a break' : "Break's over: ready for the next round?")
        store.focus('sync')
      }
    }
    tick()
    const i = setInterval(tick, 1000)
    return () => clearInterval(i)
  })

  $effect(() => {
    document.title = running ? `${mmss(store.focusRemaining(clock.now))} · ${PHASE_LABEL[store.focusTimer.phase]}` : 'Streamline'
  })
</script>
