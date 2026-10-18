<script lang="ts">
  // The week (D-94): a planner (day columns of tasks) or a calendar (an hour grid),
  // switchable and remembered.
  import Icon from '../lib/components/Icon.svelte'
  import WeekCalendar from '../lib/components/WeekCalendar.svelte'
  import WeekPlanner from '../lib/components/WeekPlanner.svelte'
  import { addDays, longDate, weekOf } from '../lib/dates'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'

  let { start: startProp = null }: { start?: string | null } = $props()
  const start = $derived(weekOf(startProp ?? store.today, store.me?.week_start ?? 1))
  const days = $derived(Array.from({ length: 7 }, (_, i) => addDays(start, i)))
  const mode = $derived(store.pref<'planner' | 'calendar'>('weekView', 'planner'))
  const thisWeek = $derived(weekOf(store.today, store.me?.week_start ?? 1))
  const go = (d: string | null) => router.go(d && d !== thisWeek ? `/week/${d}` : '/week')
</script>

<header class="head">
  <div>
    <h1>Week</h1>
    <p class="muted range">{longDate(days[0])} – {longDate(days[6])}</p>
  </div>
  <div class="tools">
    <div class="seg" role="radiogroup" aria-label="Week view">
      <button role="radio" aria-checked={mode === 'planner'} class:on={mode === 'planner'} onclick={() => store.setPref('weekView', 'planner')}
        ><Icon name="columns" size={15} /> Planner</button>
      <button role="radio" aria-checked={mode === 'calendar'} class:on={mode === 'calendar'} onclick={() => store.setPref('weekView', 'calendar')}
        ><Icon name="calendar" size={15} /> Calendar</button>
    </div>
    <a class="btn small" href="/week-template" title="Week templates: the blocks each weekday gets"><Icon name="grid" size={14} /> Templates</a>
    <nav class="weeknav" aria-label="Change week">
      <button class="icon-btn" onclick={() => go(addDays(start, -7))} aria-label="Previous week"><Icon name="left" /></button>
      {#if start !== thisWeek}<button class="btn small" onclick={() => go(null)}>This week</button>{/if}
      <button class="icon-btn" onclick={() => go(addDays(start, 7))} aria-label="Next week"><Icon name="right" /></button>
    </nav>
  </div>
</header>

{#if mode === 'calendar'}
  <WeekCalendar {days} />
{:else}
  <WeekPlanner {days} />
{/if}

<style>
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    margin-bottom: 12px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .range {
    margin: 2px 0 0;
    font-size: 14px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .weeknav {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .seg {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: var(--radius-sm);
    background: var(--bg);
    border: 1px solid var(--border);
  }
  .seg button {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border-radius: var(--radius-sm);
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
</style>
