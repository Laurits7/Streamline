<script lang="ts">
  // The day summary (SPEC §6.2b): what happened, routines, events, focus, metrics and the
  // reflection, with a month calendar to browse past days.
  import { api } from '../lib/api/client'
  import type { DayGlance } from '../lib/api/types/DayGlance'
  import type { DaySummary } from '../lib/api/types/DaySummary'
  import Icon from '../lib/components/Icon.svelte'
  import Reflection from '../lib/components/Reflection.svelte'
  import { addDays, dayLabel, fmtMinutes, longDate, userLocale } from '../lib/dates'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'
  import { moodEmoji, showWeight } from '../lib/tracking'
  import { timeRange, eventsOn } from '../lib/calendar'

  let { date: dateProp = null }: { date?: string | null } = $props()
  const date = $derived(dateProp ?? store.today)
  const go = (d: string) => router.go(d === store.today ? '/summary' : `/summary/${d}`)

  let summary = $state<DaySummary | null>(null)
  // Refetch when the day changes or anything in the store moves (cheap; one request).
  $effect(() => {
    const d = date
    void store.tasks.size
    void store.metricEntries.size
    api.get<DaySummary>(`/days/${d}/summary`).then((s) => d === date && (summary = s)).catch(() => {})
  })

  // Month calendar.
  let month = $state('')
  $effect(() => {
    month = date.slice(0, 7)
  })
  let glance = $state<DayGlance[]>([])
  const monthDays = $derived.by(() => {
    if (!month) return [] as string[]
    const [y, m] = month.split('-').map(Number)
    const n = new Date(Date.UTC(y, m, 0)).getUTCDate()
    return Array.from({ length: n }, (_, i) => `${month}-${String(i + 1).padStart(2, '0')}`)
  })
  $effect(() => {
    const days = monthDays
    if (!days.length) return
    api.get<DayGlance[]>(`/summaries?from=${days[0]}&to=${days[days.length - 1]}`).then((g) => (glance = g)).catch(() => {})
  })
  const lead = $derived.by(() => {
    if (!monthDays.length) return 0
    const iso = ((new Date(monthDays[0] + 'T00:00:00Z').getUTCDay() + 6) % 7) + 1
    return (iso + 7 - (store.me?.week_start ?? 1)) % 7
  })
  const monthLabel = $derived(
    month ? new Date(month + '-01T00:00:00Z').toLocaleDateString(userLocale(), { month: 'long', year: 'numeric', timeZone: 'UTC' }) : '',
  )
  const shiftMonth = (n: number) => {
    const [y, m] = month.split('-').map(Number)
    const d = new Date(Date.UTC(y, m - 1 + n, 1))
    month = d.toISOString().slice(0, 7)
  }
  const weekdays = $derived.by(() => {
    const ws = store.me?.week_start ?? 1
    return Array.from({ length: 7 }, (_, i) =>
      new Date(Date.UTC(2024, 0, ((ws - 1 + i) % 7) + 1)).toLocaleDateString(userLocale(), { weekday: 'narrow', timeZone: 'UTC' }),
    )
  })

  const imperial = $derived(store.me?.unit_system === 'imperial')
  function metricText(id: string, v: number) {
    const m = store.metrics.get(id)
    if (!m) return String(v)
    if (m.key === 'mood') return `${moodEmoji(v)} ${Math.round(v * 10) / 10}/5`
    if (m.key === 'weight') return `${Math.round(showWeight(v, imperial) * 10) / 10} ${imperial ? 'lb' : 'kg'}`
    if (m.kind === 'yes_no') return v ? 'yes' : 'no'
    return `${Math.round(v * 10) / 10}${m.unit ? ' ' + m.unit : ''}`
  }
  const events = $derived(summary ? eventsOn(date, summary.events, store.me?.timezone ?? 'UTC', (id) => store.calendars.get(id)) : null)
</script>

<header class="head">
  <div>
    <h1>{dayLabel(date, store.today)}</h1>
    <p class="muted">{longDate(date)} · <a href="/trends">Trends</a></p>
  </div>
  <nav class="daynav" aria-label="Change day">
    <button class="icon-btn" onclick={() => go(addDays(date, -1))} aria-label="Previous day"><Icon name="left" /></button>
    {#if date !== store.today}<button class="btn small" onclick={() => go(store.today)}>Today</button>{/if}
    <button class="icon-btn" onclick={() => go(addDays(date, 1))} aria-label="Next day" disabled={date >= store.today}><Icon name="right" /></button>
  </nav>
</header>

<div class="layout">
  <div class="sections">
      <section class="card stats">
        <div><strong>{summary ? `${summary.done}/${summary.planned}` : '–'}</strong><span>planned done</span></div>
        <div><strong>{summary?.completion_rate == null ? '–' : `${Math.round(summary.completion_rate * 100)}%`}</strong><span>completion</span></div>
        <div><strong>{summary?.completed_total ?? '–'}</strong><span>completed in all</span></div>
        <div><strong>{summary ? fmtMinutes(summary.focus_min) : '–'}</strong><span>focus</span></div>
      </section>
      <section class="card">
        <h2>Tasks</h2>
        <ul class="facts">
          <li>Done <strong>{summary?.done ?? '–'}</strong></li>
          <li>Carried over <strong>{summary?.carried ?? '–'}</strong></li>
          <li>Missed <strong>{summary?.missed ?? '–'}</strong></li>
          <li>Skipped <strong>{summary?.skipped ?? '–'}</strong></li>
          <li>Still open <strong>{summary?.open ?? '–'}</strong></li>
          <li>Workflow steps done <strong>{summary?.steps_done ?? '–'}</strong></li>
          <li>Estimated vs. recorded <strong>{summary?.estimated_min ? `${fmtMinutes(summary.estimated_min)} / ${fmtMinutes(summary.actual_min)}` : '–'}</strong></li>
        </ul>
        <a class="small" href={date === store.today ? '/' : `/day/${date}`}>Open the day →</a>
      </section>
    <section class="card reflection-card">
      <h2>Reflection</h2>
      {#key date}<Reflection {date} />{/key}
    </section>
    {#if summary}
      {#if summary.routines.length}
        <section class="card">
          <h2>Routines</h2>
          <ul class="facts">
            {#each summary.routines as r (r.series_id)}
              <li><span class:ok={r.done}>{r.done ? '✓' : '○'} {r.title}</span>{#if r.streak > 1}<span class="muted small">🔥 {r.streak}</span>{/if}</li>
            {/each}
          </ul>
        </section>
      {/if}
      {#if events && (events.allDay.length || events.timed.length)}
        <section class="card">
          <h2>Events</h2>
          <ul class="facts">
            {#each [...events.allDay, ...events.timed] as d (d.event.id)}
              <li><span><span class="dot" style:background={d.color}></span> {d.event.title}</span><span class="muted small">{timeRange(d)}</span></li>
            {/each}
          </ul>
        </section>
      {/if}
      {#if summary.metrics.length}
        <section class="card">
          <h2>Tracked</h2>
          <ul class="facts">
            {#each summary.metrics as m (m.metric_id)}
              {@const def = store.metrics.get(m.metric_id)}
              <li>
                <span>
                  {def?.name}
                  {#if def?.key === 'mood' && m.entries > 1}
                    <span class="moods muted small">
                      {#each store.entriesOf(m.metric_id).filter((e) => e.date === date).sort((x, y) => (x.at < y.at ? -1 : 1)) as e (e.id)}
                        <span>{new Date(e.at).toLocaleTimeString(userLocale(), { hour: '2-digit', minute: '2-digit', timeZone: store.me?.timezone })} {moodEmoji(e.value)}</span>
                      {/each}
                    </span>
                  {/if}
                </span>
                <strong>{def?.key === 'mood' && m.entries > 1 ? `avg ${metricText(m.metric_id, m.value)}` : metricText(m.metric_id, m.value)}</strong>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}

  </div>

  <aside class="card month">
    <div class="mhead">
      <button class="icon-btn" onclick={() => shiftMonth(-1)} aria-label="Previous month"><Icon name="left" size={16} /></button>
      <strong>{monthLabel}</strong>
      <button class="icon-btn" onclick={() => shiftMonth(1)} aria-label="Next month"><Icon name="right" size={16} /></button>
    </div>
    <div class="grid">
      {#each weekdays as w, i (i)}<span class="wd">{w}</span>{/each}
      {#each Array.from({ length: lead }) as _, i (i)}<span></span>{/each}
      {#each monthDays as d (d)}
        {@const g = glance.find((x) => x.date === d)}
        <button class="cell" class:sel={d === date} class:future={d > store.today} disabled={d > store.today} onclick={() => go(d)} aria-label={longDate(d)}>
          <span class="num">{Number(d.slice(8))}</span>
          {#if g?.mood}<span class="emo">{moodEmoji(g.mood)}</span>{/if}
          {#if g && g.planned}<span class="bar"><span style:width="{(g.done / g.planned) * 100}%"></span></span>{/if}
          {#if g?.journal}<span class="jd" title="Journal"></span>{/if}
        </button>
      {/each}
    </div>
  </aside>
</div>

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 14px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .daynav {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .sections {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 0 12px;
    align-items: start;
  }
  .sections .stats,
  .reflection-card {
    grid-column: 1 / -1;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    gap: 14px;
    align-items: start;
  }
  @media (max-width: 900px) {
    .layout {
      grid-template-columns: 1fr;
    }
    .month {
      order: -1;
    }
  }
  section {
    padding: 14px 16px;
    margin-bottom: 12px;
  }
  h2 {
    font-size: 15px;
    margin-bottom: 8px;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    text-align: center;
  }
  .stats strong {
    display: block;
    font-size: 22px;
  }
  .stats span {
    font-size: 12px;
    color: var(--muted);
  }
  .facts {
    list-style: none;
    padding: 0;
    margin: 0 0 6px;
  }
  .facts li {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 4px 0;
    font-size: 14px;
    border-bottom: 1px solid var(--border);
  }
  .moods {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 2px 8px;
    margin-left: 6px;
  }
  .ok {
    color: var(--ok);
  }
  .dot {
    display: inline-block;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    margin-right: 4px;
  }
  .small {
    font-size: 12px;
  }
  .month {
    padding: 12px;
  }
  .mhead {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 3px;
  }
  .wd {
    text-align: center;
    font-size: 11px;
    color: var(--faint);
  }
  .cell {
    position: relative;
    aspect-ratio: 1;
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    background: var(--bg);
  }
  .cell.sel {
    outline: 2px solid var(--accent);
  }
  .cell.future {
    opacity: 0.4;
  }
  .num {
    font-weight: 600;
  }
  .emo {
    font-size: 11px;
    line-height: 1;
  }
  .bar {
    position: absolute;
    left: 6px;
    right: 6px;
    bottom: 4px;
    height: 3px;
    background: var(--border);
    border-radius: 2px;
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--ok);
  }
  .jd {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent);
  }
</style>
