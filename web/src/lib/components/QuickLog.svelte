<script lang="ts">
  // Today's quick log (SPEC §6.2b): mood, weight and your own metrics in a tap or two.
  import type { MetricDefinition } from '../api/types/MetricDefinition'
  import { store } from '../store.svelte'
  import { MOODS, moodEmoji, showWeight, storeWeight } from '../tracking'
  import { toast } from '../toast.svelte'
  import Icon from './Icon.svelte'

  let { date }: { date: string } = $props()
  const imperial = $derived(store.me?.unit_system === 'imperial')
  const metrics = $derived(store.metricList())
  const todays = (m: MetricDefinition) =>
    store
      .entriesOf(m.id)
      .filter((e) => e.date === date)
      .sort((a, b) => (a.at < b.at ? -1 : 1))
  const clock = (at: string) =>
    new Date(at).toLocaleTimeString(store.me?.locale || undefined, { hour: '2-digit', minute: '2-digit', timeZone: store.me?.timezone })
  // Mood changes through the day: every tap is a new entry ("how I feel now").
  async function logMood(m: MetricDefinition, v: number) {
    const before = new Set(todays(m).map((e) => e.id))
    await store.logMetric(m.id, v, date)
    const added = todays(m).find((e) => !before.has(e.id))
    toast(`Mood ${MOODS[v - 1]} logged`, 'info', added ? { label: 'Undo', run: () => store.deleteMetricEntry(added.id) } : undefined)
  }
  const valueOf = (m: MetricDefinition) => store.metricDaily(m.id).get(date)

  let drafts = $state<Record<string, string>>({})
  function logNumber(e: Event, m: MetricDefinition) {
    e.preventDefault()
    const raw = (drafts[m.id] ?? '').replace(',', '.')
    const n = Number(raw)
    if (!raw.trim() || !Number.isFinite(n)) return
    store.logMetric(m.id, m.key === 'weight' ? Math.round(storeWeight(n, imperial) * 1000) / 1000 : n, date)
    drafts[m.id] = ''
  }
  const unitOf = (m: MetricDefinition) => (m.key === 'weight' ? (imperial ? 'lb' : 'kg') : m.unit)
  const fmt = (m: MetricDefinition, v: number) => {
    const shown = m.key === 'weight' ? showWeight(v, imperial) : v
    return `${Math.round(shown * 10) / 10}${unitOf(m) ? ' ' + unitOf(m) : ''}`
  }
</script>

<section class="card quicklog" aria-label="Track">
  <header>
    <h2 class="section-title"><Icon name="chart" size={14} /> Track</h2>
    <a href="/summary/{date}" class="link">Summary &amp; journal <Icon name="right" size={14} /></a>
  </header>
  {#each metrics as m (m.id)}
    {@const v = valueOf(m)}
    <div class="metric">
      <span class="name">{m.name}</span>
      {#if m.key === 'mood'}
        <div class="moods" role="group" aria-label="Mood">
          {#each MOODS as emoji, i (emoji)}
            <button class="mood" aria-label="Mood {i + 1} of 5: log it now" onclick={() => logMood(m, i + 1)}>{emoji}</button>
          {/each}
        </div>
      {:else if m.kind === 'scale'}
        <div class="scale" role="group" aria-label={m.name}>
          {#each Array.from({ length: (m.scale_max ?? 5) - (m.scale_min ?? 1) + 1 }, (_, i) => (m.scale_min ?? 1) + i) as n (n)}
            <button class="step" class:on={v !== undefined && Math.round(v) === n} onclick={() => store.logMetric(m.id, n, date)}>{n}</button>
          {/each}
        </div>
      {:else if m.kind === 'yes_no'}
        <div class="scale">
          <button class="step" class:on={v === 1} onclick={() => store.logMetric(m.id, 1, date)}>Yes</button>
          <button class="step" class:on={v === 0} onclick={() => store.logMetric(m.id, 0, date)}>No</button>
        </div>
      {:else}
        <form class="num" onsubmit={(e) => logNumber(e, m)}>
          <input type="text" inputmode="decimal" placeholder={v !== undefined ? fmt(m, v) : unitOf(m) || 'value'} bind:value={drafts[m.id]} aria-label="{m.name} ({unitOf(m)})" />
          <button class="btn small" type="submit">Log</button>
        </form>
      {/if}
      <span class="today muted">
        {#if v !== undefined && m.key === 'mood'}avg {Math.round(v * 10) / 10}/5{:else if v !== undefined}{m.kind === 'yes_no' ? (v ? 'yes' : 'no') : fmt(m, v)}{#if todays(m).length > 1} · {todays(m).length}×{/if}{/if}
      </span>
      {#if m.key === 'mood' && todays(m).length}
        <div class="moodlog" aria-label="Mood today">
          {#each todays(m) as e (e.id)}
            <span class="entry">
              <span class="muted">{clock(e.at)}</span> {moodEmoji(e.value)}
              <button class="del" aria-label="Remove mood logged at {clock(e.at)}" onclick={() => store.deleteMetricEntry(e.id)}>×</button>
            </span>
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</section>

<style>
  .quicklog {
    padding: 12px 14px;
    margin-bottom: 14px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: 13px;
  }
  .metric {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    padding: 5px 0;
  }
  .name {
    min-width: 90px;
    font-size: 14px;
    font-weight: 600;
  }
  .moods,
  .scale {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .mood {
    font-size: 22px;
    line-height: 1;
    padding: 4px;
    border-radius: 10px;
    filter: grayscale(0.7);
    opacity: 0.7;
  }
  .mood:active {
    transform: scale(1.2);
  }
  .moodlog {
    flex-basis: 100%;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    padding-left: 100px;
    font-size: 13px;
  }
  .entry {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .del {
    color: var(--faint);
    padding: 0 3px;
  }
  @media (max-width: 560px) {
    .moodlog {
      padding-left: 0;
    }
  }
  .mood:hover {
    filter: none;
    opacity: 1;
    background: var(--accent-soft);
  }
  .step {
    min-width: 32px;
    padding: 4px 8px;
    border-radius: 8px;
    border: 1px solid var(--border);
    font-size: 13px;
  }
  .step.on {
    background: var(--accent);
    color: var(--accent-text);
    border-color: var(--accent);
  }
  .num {
    display: flex;
    gap: 6px;
  }
  .num input {
    width: 110px;
    padding: 4px 8px;
  }
  .today {
    margin-left: auto;
    font-size: 12px;
  }
</style>
