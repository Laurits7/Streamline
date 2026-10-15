<script lang="ts">
  import { accuracy } from '../lib/estimates'
  import { weekOf } from '../lib/dates'
  // Trends (SPEC §6.2b): each metric over a week, month or year, and mood next to
  // routine completion. Small SVG charts, no chart library.
  import Icon from '../lib/components/Icon.svelte'
  import LineChart from '../lib/components/LineChart.svelte'
  import { addDays, userLocale } from '../lib/dates'
  import { store } from '../lib/store.svelte'
  import { showWeight, trend, type Range } from '../lib/tracking'
  import { healthLabel, STATUSES } from '../lib/health'

  let range = $state<Range>('month')
  const metrics = $derived(store.metricList())
  let selected = $state<string | null>(null)
  const metric = $derived(store.metrics.get(selected ?? '') ?? metrics[0])
  const imperial = $derived(store.me?.unit_system === 'imperial')

  const label = (d: string) =>
    new Date(d + 'T00:00:00Z').toLocaleDateString(userLocale(), range === 'week' ? { weekday: 'short', timeZone: 'UTC' } : { day: 'numeric', month: 'short', timeZone: 'UTC' })
  const points = $derived(metric ? trend(store.metricDaily(metric.id), store.today, range) : [])
  const values = $derived(points.map((p) => (p.value === null ? null : metric?.key === 'weight' ? showWeight(p.value, imperial) : p.value)))
  const unit = $derived(metric?.key === 'weight' ? (imperial ? 'lb' : 'kg') : (metric?.unit ?? ''))
  const logged = $derived(values.filter((v): v is number => v !== null))
  const avg = $derived(logged.length ? logged.reduce((a, b) => a + b, 0) / logged.length : null)

  // Mood vs. routine completion (the last 30 days are on this device).
  const mood = $derived(store.metricByKey('mood'))
  const span = $derived(range === 'week' ? 7 : 30)
  const days = $derived(Array.from({ length: span }, (_, i) => addDays(store.today, i - span + 1)))
  const routineRate = $derived(
    days.map((d) => {
      const occ = [...store.tasks.values()].filter((t) => t.series_id && !t.window_end && t.occurrence_date === d && t.status !== 'open')
      return occ.length ? (occ.filter((t) => t.status === 'done').length / occ.length) * 100 : null
    }),
  )
  const moodByDay = $derived(mood ? store.metricDaily(mood.id) : new Map<string, number>())
  // Health check-ins (D-75): one square per day, and how many days of each.
  const healthSpan = $derived(range === 'week' ? 7 : range === 'month' ? 30 : 364)
  const healthDays = $derived(Array.from({ length: healthSpan }, (_, i) => addDays(store.today, i - healthSpan + 1)))
  const healthBy = $derived(new Map([...store.healthDays.values()].map((h) => [h.date, h])))
  const healthCounts = $derived(
    STATUSES.map((s) => ({ ...s, n: healthDays.filter((d) => healthBy.get(d)?.status === s.id).length })).filter((s) => s.n),
  )
  const moodPct = $derived(days.map((d) => (moodByDay.has(d) ? ((moodByDay.get(d)! - 1) / 4) * 100 : null)))
  // Estimates against focus time actually spent (10.5).
  const est = $derived(accuracy(store.tasks.values(), (iso) => weekOf(iso.slice(0, 10), store.me?.week_start ?? 1)))
  const projectFactors = $derived(
    [...store.estimateModel.byProject.entries()]
      .map(([id, f]) => [id ? store.projectPath(id) : 'Inbox', f] as const)
      .sort((a, b) => b[1].n - a[1].n)
      .slice(0, 5),
  )
</script>

<h1>Trends</h1>
<div class="bar">
  <div class="chips" role="group" aria-label="Metric">
    {#each metrics as m (m.id)}
      <button class="chip" class:on={metric?.id === m.id} onclick={() => (selected = m.id)}>{m.name}</button>
    {/each}
  </div>
  <div class="chips" role="group" aria-label="Range">
    {#each [['week', 'Week'], ['month', 'Month'], ['year', 'Year']] as [r, l] (r)}
      <button class="chip" class:on={range === r} onclick={() => (range = r as Range)}>{l}</button>
    {/each}
  </div>
</div>

<div class="charts">
{#if metric}
  <section class="card">
    <h2>{metric.name}{unit ? ` (${unit})` : ''}</h2>
    {#if logged.length}
      <LineChart
        labels={points.map((p) => label(p.start))}
        series={[{ label: metric.name, color: 'var(--accent)', points: values, ...(metric.kind === 'scale' ? { min: metric.scale_min ?? 1, max: metric.scale_max ?? 5 } : metric.kind === 'yes_no' ? { min: 0, max: 1 } : {}) }]} />
      <p class="muted small">
        {logged.length} {range === 'year' ? 'weeks' : 'days'} logged{avg !== null ? ` · average ${Math.round(avg * 10) / 10}${unit ? ' ' + unit : ''}` : ''}{range === 'year' ? ' · weekly averages' : ''}
      </p>
    {:else}
      <p class="muted">Nothing logged in this period. Log it from the <a href="/">Today</a> view.</p>
    {/if}
  </section>
{/if}

{#if mood}
  <section class="card">
    <h2>Mood and routines</h2>
    {#if moodPct.some((v) => v !== null) || routineRate.some((v) => v !== null)}
      <LineChart
        labels={days.map(label)}
        format={(v) => `${Math.round(v)}%`}
        series={[
          { label: 'Mood', color: 'var(--accent)', points: moodPct, min: 0, max: 100 },
          { label: 'Routines done', color: 'var(--ok)', points: routineRate, min: 0, max: 100 },
        ]} />
      <p class="legend small"><span class="sw" style:background="var(--accent)"></span> Mood (😞 0% … 😄 100%) <span class="sw" style:background="var(--ok)"></span> Routines done that day</p>
    {:else}
      <p class="muted">Log your mood for a few days to see it next to your routines.</p>
    {/if}
  </section>
{/if}

<section class="card">
  <h2>Estimates</h2>
  {#if est.n}
    <p class="lead-num"><strong>{est.within === null ? '–' : Math.round(est.within * 100)}%</strong> of {est.n} finished tasks took within a quarter of their estimate{store.estimateModel.overall ? `; on the whole they take ${store.estimateModel.overall.factor.toFixed(1)}× the estimate` : ''}.</p>
    <div class="weeks" role="img" aria-label="How long tasks took against their estimate, per week">
      {#each est.weeks as w (w.week)}
        <div class="wk" title="Week of {w.week}: {w.factor.toFixed(2)}× ({w.n} tasks)">
          <span class="b" class:over={w.factor > 1.25} class:under={w.factor < 0.75} style:height="{Math.min(100, (w.factor / 2) * 100)}%"></span>
          <span class="l">{w.week.slice(5)}</span>
        </div>
      {/each}
    </div>
    <p class="small muted">Per week: how long tasks took against their estimate. Half height is right on; orange means over by more than a quarter, grey under.</p>
    {#if projectFactors.length}
      <p class="small muted">By project: {projectFactors.map(([name, f]) => `${name} ${f.factor.toFixed(1)}× (${f.n})`).join(' · ')}</p>
    {/if}
  {:else}
    <p class="muted small">Give tasks an estimate and use the focus timer on them: once a few are done, this shows how long things really take.</p>
  {/if}
</section>
{#if store.healthDays.size}
  <section class="card">
    <h2>Health</h2>
    <div class="hstrip" class:year={range === 'year'} style:--cols={range === 'year' ? 52 : healthSpan} role="img" aria-label="Health, the last {healthSpan} days">
      {#each healthDays as d (d)}
        {@const h = healthBy.get(d)}
        <span class="hd {h?.status ?? 'none'}" title="{label(d)}: {h ? healthLabel(h) : 'nothing logged'}"></span>
      {/each}
    </div>
    <p class="legend small">
      {#each healthCounts as c (c.id)}<span><span class="hd {c.id}"></span> {c.label}: {c.n} {c.n === 1 ? 'day' : 'days'}</span>{:else}Nothing logged in this period.{/each}
    </p>
  </section>
{/if}
</div>
<p class="muted small"><Icon name="settings" size={12} /> Add your own metrics (sleep, water, steps…) in <a href="/settings">Settings</a>.</p>

<style>
  .lead-num {
    margin: 0 0 10px;
  }
  .lead-num strong {
    font-size: 22px;
  }
  .weeks {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    height: 90px;
    margin-bottom: 6px;
  }
  .wk {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    height: 100%;
    gap: 2px;
  }
  .wk .b {
    width: 100%;
    max-width: 28px;
    border-radius: 4px 4px 0 0;
    background: var(--accent);
  }
  .wk .b.over {
    background: var(--warn);
  }
  .wk .b.under {
    background: var(--faint);
  }
  .wk .l {
    font-size: 10px;
    color: var(--muted);
  }
  .charts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(520px, 1fr));
    gap: 0 12px;
    align-items: start;
  }
  @media (max-width: 600px) {
    .charts {
      grid-template-columns: 1fr;
    }
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
    margin-bottom: 12px;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    font-size: 13px;
    padding: 4px 12px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface);
  }
  .chip.on {
    background: var(--accent);
    color: var(--accent-text);
    border-color: var(--accent);
  }
  section {
    padding: 14px 16px;
    margin-bottom: 12px;
  }
  h2 {
    font-size: 15px;
    margin-bottom: 8px;
  }
  .small {
    font-size: 12px;
  }
  .legend {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    color: var(--muted);
  }
  .hstrip {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    gap: 3px;
    max-width: calc(var(--cols) * 28px);
    margin-bottom: 8px;
  }
  /* A year: one column per week. */
  .hstrip.year {
    grid-template-rows: repeat(7, auto);
    grid-auto-flow: column;
    gap: 2px;
  }
  .hd {
    display: inline-block;
    aspect-ratio: 1;
    border-radius: 2px;
    background: var(--surface-2);
  }
  .legend .hd {
    width: 10px;
    vertical-align: -1px;
  }
  .hd.none {
    background: transparent;
    box-shadow: inset 0 0 0 1px var(--border);
  }
  .hd.great {
    background: var(--ok);
  }
  .hd.ok {
    background: color-mix(in srgb, var(--ok) 40%, var(--surface-2));
  }
  .hd.unwell {
    background: color-mix(in srgb, var(--warn) 45%, var(--surface-2));
  }
  .hd.sick {
    background: var(--warn);
  }
  .hd.injured {
    background: var(--danger);
  }
  .sw {
    display: inline-block;
    width: 14px;
    height: 3px;
    border-radius: 2px;
  }
</style>
