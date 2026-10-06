<script lang="ts">
  // Planned time against free time for a day; warns (but never blocks) when overbooked (D-9).
  import { fmtMinutes, nowHHMM } from '../dates'
  import { dayLoad } from '../planning'
  import { store } from '../store.svelte'

  let { date }: { date: string } = $props()
  const load = $derived(
    dayLoad(store.me!, store.dayEntries(date), (id) => store.tasks.get(id), date === store.today ? nowHHMM(store.me!.timezone) : undefined),
  )
  const over = $derived(load.planned - load.free)
  const pct = $derived(load.free > 0 ? Math.min(100, (load.planned / load.free) * 100) : load.planned > 0 ? 100 : 0)
</script>

<div class="load" class:over={over > 0} role="status" aria-live="polite">
  <div class="text">
    <span><strong>{fmtMinutes(load.planned)}</strong> planned</span>
    <span class="muted">of {fmtMinutes(load.free)} free{load.scheduled ? ` (after ${load.scheduled} scheduled)` : ''}</span>
  </div>
  <div class="bar"><div style:width="{pct}%"></div></div>
  {#if over > 0}
    <p class="warn">Overbooked by {fmtMinutes(over)}. Consider moving something to another day.</p>
  {/if}
  {#if load.unestimated}
    <p class="muted note">{load.unestimated} task{load.unestimated === 1 ? ' has' : 's have'} no estimate and {load.unestimated === 1 ? "isn't" : "aren't"} counted.</p>
  {/if}
</div>

<style>
  .load {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 14px;
  }
  .text {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    font-size: 14px;
    margin-bottom: 6px;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 200ms ease;
  }
  .over .bar div {
    background: var(--warn);
  }
  .warn {
    color: var(--warn);
    font-weight: 600;
    font-size: 13px;
    margin: 6px 0 0;
  }
  .note {
    font-size: 12px;
    margin: 4px 0 0;
  }
</style>
