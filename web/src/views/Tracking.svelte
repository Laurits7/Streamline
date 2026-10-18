<script lang="ts">
  // Tracking (D-108): everything you track for one day, with each metric's entries and a
  // week strip, plus health and habits. What shows (and in what order) is set in
  // Settings → Tracking; Today's Track card uses the same widgets.
  import type { MetricDefinition } from '../lib/api/types/MetricDefinition'
  import HabitCheckins from '../lib/components/HabitCheckins.svelte'
  import HealthPicker from '../lib/components/HealthPicker.svelte'
  import Icon from '../lib/components/Icon.svelte'
  import MetricLogger from '../lib/components/MetricLogger.svelte'
  import { addDays, dayLabel, longDate, userLocale } from '../lib/dates'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'
  import { moodEmoji, valueText } from '../lib/tracking'

  let { date: dateProp = null }: { date?: string | null } = $props()
  // Not beyond today: tracking is about what happened.
  const date = $derived(dateProp && dateProp < store.today ? dateProp : store.today)
  const go = (d: string) => router.go(d >= store.today ? '/tracking' : `/tracking/${d}`)
  const imperial = $derived(store.me?.unit_system === 'imperial')
  const metrics = $derived(store.metricList())
  // This device keeps the last 30 days (older days are on the summary).
  const old = $derived(date < addDays(store.today, -30))

  const week = $derived(Array.from({ length: 7 }, (_, i) => addDays(date, i - 6)))
  const weekday = (d: string) => new Date(d + 'T00:00:00Z').toLocaleDateString(userLocale(), { weekday: 'narrow', timeZone: 'UTC' })
  /** The week's bars, 0-100 (null: nothing logged). */
  function bars(m: MetricDefinition, values: Map<string, number>): (number | null)[] {
    const vals = week.map((d) => values.get(d) ?? null)
    if (m.kind === 'yes_no') return vals.map((v) => (v === null ? null : v ? 100 : 8))
    if (m.kind === 'scale') {
      const [lo, hi] = [m.scale_min ?? 1, m.scale_max ?? 5]
      return vals.map((v) => (v === null ? null : 8 + ((v - lo) / Math.max(1, hi - lo)) * 92))
    }
    const top = Math.max(...vals.map((v) => Math.abs(v ?? 0)))
    return vals.map((v) => (v === null ? null : top ? 8 + (Math.abs(v) / top) * 92 : 8))
  }
  const dayValue = (m: MetricDefinition, v: number) => (m.key === 'mood' ? `${moodEmoji(v)} ${Math.round(v * 10) / 10}/5` : valueText(m, v, imperial))
</script>

<header class="head">
  <div>
    <h1>Tracking</h1>
    <p class="muted">{dayLabel(date, store.today)} · {longDate(date)} · <a href="/trends">Trends</a> · <a href="/settings#tracking">Choose what to track</a></p>
  </div>
  <nav class="daynav" aria-label="Change day">
    <button class="icon-btn" onclick={() => go(addDays(date, -1))} aria-label="Previous day"><Icon name="left" /></button>
    {#if date !== store.today}<button class="btn small" onclick={() => go(store.today)}>Today</button>{/if}
    <button class="icon-btn" onclick={() => go(addDays(date, 1))} aria-label="Next day" disabled={date >= store.today}><Icon name="right" /></button>
  </nav>
</header>

{#if old}
  <p class="muted small note">Days more than a month back aren't kept on this device, so earlier entries may be missing here; the <a href="/summary/{date}">summary</a> has them.</p>
{/if}

<div class="cards">
  {#each metrics as m (m.id)}
    {@const values = store.metricDaily(m.id)}
    {@const v = values.get(date)}
    <section class="card metric" aria-label={m.name}>
      <header class="mhead">
        <h2>{#if m.emoji}<span class="emoji" aria-hidden="true">{m.emoji}</span>{/if}{m.name}</h2>
        {#if v !== undefined}<strong class="value">{dayValue(m, v)}</strong>{/if}
      </header>
      <div class="logger">
        <MetricLogger metric={m} {date} full />
      </div>
      <div class="strip" role="img" aria-label="{m.name}, the 7 days up to {longDate(date)}">
        {#each bars(m, values) as h, i (week[i])}
          {@const d = week[i]}
          {@const dv = values.get(d)}
          <button class="day" class:sel={d === date} title="{longDate(d)}: {dv === undefined ? 'nothing logged' : dayValue(m, dv)}" aria-label="{longDate(d)}" onclick={() => go(d)}>
            <span class="track"><span class="bar" class:none={h === null} style:height="{h ?? 4}%"></span></span>
            <span class="l">{weekday(d)}</span>
          </button>
        {/each}
      </div>
    </section>
  {:else}
    <p class="muted">Nothing to track yet. <a href="/settings#tracking">Choose what to track</a>.</p>
  {/each}
  <section class="card">
    <h2>Health</h2>
    <HealthPicker {date} />
  </section>
  {#if store.habitsFor(date).length}
    <section class="card">
      <h2>Habits</h2>
      <HabitCheckins {date} />
    </section>
  {/if}
</div>

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 10px;
    margin-bottom: 14px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
  }
  .daynav {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .note {
    margin: 0 0 12px;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 340px), 1fr));
    gap: 12px;
    align-items: start;
  }
  .cards > :global(.card) {
    padding: 14px 16px;
    margin: 0;
  }
  h2 {
    font-size: 16px;
    margin: 0 0 8px;
  }
  .mhead {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }
  .emoji {
    margin-right: 6px;
  }
  .value {
    font-size: 18px;
    white-space: nowrap;
  }
  .logger {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    --log-indent: 0;
  }
  .strip {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 4px;
    margin-top: 12px;
  }
  .day {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 2px;
    padding: 2px;
    border-radius: 6px;
  }
  .day.sel {
    background: var(--accent-soft);
  }
  .track {
    height: 36px;
    display: flex;
    align-items: flex-end;
  }
  .bar {
    width: 100%;
    border-radius: 3px;
    background: var(--accent);
  }
  .bar.none {
    background: var(--border);
  }
  .l {
    font-size: 11px;
    color: var(--muted);
    text-align: center;
  }
</style>
