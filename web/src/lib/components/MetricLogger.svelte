<script lang="ts">
  // Logging one metric for a day (D-108): mood faces, scale steps, yes/no, quick amounts
  // ("+200 ml", each a new entry) and a free number. Used by Today's Track card (compact)
  // and the Tracking page (full: every entry of the day, with delete).
  import type { MetricDefinition } from '../api/types/MetricDefinition'
  import { store } from '../store.svelte'
  import { MOODS, moodEmoji, presetLabel, storeWeight, unitOf, valueText } from '../tracking'
  import { toast } from '../toast.svelte'

  let { metric: m, date, full = false }: { metric: MetricDefinition; date: string; full?: boolean } = $props()
  const imperial = $derived(store.me?.unit_system === 'imperial')
  const unit = $derived(unitOf(m, imperial))
  const entries = $derived(
    store
      .entriesOf(m.id)
      .filter((e) => e.date === date)
      .sort((a, b) => (a.at < b.at ? -1 : 1)),
  )
  const v = $derived(store.metricDaily(m.id).get(date))
  const clock = (at: string) =>
    new Date(at).toLocaleTimeString(store.me?.locale || undefined, { hour: '2-digit', minute: '2-digit', timeZone: store.me?.timezone })

  // Mood and quick amounts add up through the day: every tap is a new entry, with an undo.
  async function logWithUndo(value: number, what: string) {
    const before = new Set(entries.map((e) => e.id))
    await store.logMetric(m.id, value, date)
    const added = entries.find((e) => !before.has(e.id))
    toast(`${what} logged`, 'info', added ? { label: 'Undo', run: () => store.deleteMetricEntry(added.id) } : undefined)
  }

  let draft = $state('')
  function logNumber(e: Event) {
    e.preventDefault()
    const raw = draft.replace(',', '.')
    const n = Number(raw)
    if (!raw.trim() || !Number.isFinite(n)) return
    store.logMetric(m.id, m.key === 'weight' ? Math.round(storeWeight(n, imperial) * 1000) / 1000 : n, date)
    draft = ''
  }
  const entryText = (value: number) => (m.key === 'mood' ? moodEmoji(value) : valueText(m, value, imperial))
  const showEntries = $derived(entries.length > 0 && (full || m.key === 'mood'))
</script>

{#if m.key === 'mood'}
  <div class="moods" role="group" aria-label="Mood">
    {#each MOODS as emoji, i (emoji)}
      <button class="mood" aria-label="Mood {i + 1} of 5: log it now" onclick={() => logWithUndo(i + 1, `Mood ${emoji}`)}>{emoji}</button>
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
  <div class="number">
    {#if m.presets.length}
      <div class="presets" role="group" aria-label="Quick amounts for {m.name}">
        {#each m.presets as p, i (i)}
          <button class="step preset" onclick={() => logWithUndo(p, `${presetLabel(p, unit)} ${m.name.toLowerCase()}`)}>{presetLabel(p, unit)}</button>
        {/each}
      </div>
    {/if}
    <form class="num" onsubmit={logNumber}>
      <input
        type="text"
        inputmode="decimal"
        placeholder={m.presets.length ? 'other' : v !== undefined ? valueText(m, v, imperial) : unit || 'value'}
        bind:value={draft}
        aria-label="{m.name}{unit ? ` (${unit})` : ''}" />
      <button class="btn small" type="submit">Log</button>
    </form>
  </div>
{/if}
<span class="today muted">
  {#if v !== undefined && m.key === 'mood'}avg {Math.round(v * 10) / 10}/5{:else if v !== undefined}{valueText(m, v, imperial)}{#if m.aggregate === 'sum'}{' today'}{/if}{#if entries.length > 1}{` · ${entries.length}×`}{/if}{/if}
</span>
{#if showEntries}
  <div class="log" aria-label="{m.name} logged that day">
    {#each entries as e (e.id)}
      <span class="entry">
        <span class="muted">{clock(e.at)}</span> {entryText(e.value)}
        <button class="del" aria-label="Remove {m.name} logged at {clock(e.at)}" onclick={() => store.deleteMetricEntry(e.id)}>×</button>
      </span>
    {/each}
  </div>
{/if}

<style>
  .moods,
  .scale,
  .presets {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .number {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
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
  .preset:hover {
    background: var(--accent-soft);
  }
  .preset:active {
    transform: scale(1.08);
  }
  .num {
    display: flex;
    gap: 6px;
  }
  .num input {
    width: 90px;
    padding: 4px 8px;
  }
  .today {
    margin-left: auto;
    font-size: 12px;
  }
  .log {
    flex-basis: 100%;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    padding-left: var(--log-indent, 100px);
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
    .log {
      padding-left: 0;
    }
  }
</style>
