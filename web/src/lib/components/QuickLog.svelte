<script lang="ts">
  import HabitCheckins from './HabitCheckins.svelte'
  // Today's quick log (SPEC §6.2b): mood, weight, water and your own metrics in a tap or
  // two. The Tracking page has the same widgets, with each day's entries.
  import { store } from '../store.svelte'
  import Icon from './Icon.svelte'
  import HealthPicker from './HealthPicker.svelte'
  import MetricLogger from './MetricLogger.svelte'

  let { date }: { date: string } = $props()
  const metrics = $derived(store.metricList())
</script>

<section class="card quicklog" aria-label="Track" data-tour="track">
  <header>
    <h2 class="section-title"><Icon name="chart" size={14} /> Track</h2>
    <span class="links">
      <a href="/tracking" class="link">Tracking <Icon name="right" size={14} /></a>
      <a href="/summary/{date}" class="link">Summary &amp; journal <Icon name="right" size={14} /></a>
    </span>
  </header>
  <div class="metric health-row">
    <span class="name">Health</span>
    <HealthPicker {date} compact />
  </div>
  {#each metrics as m (m.id)}
    <div class="metric">
      <span class="name">{#if m.emoji}<span class="emoji" aria-hidden="true">{m.emoji}</span> {/if}{m.name}</span>
      <MetricLogger metric={m} {date} />
    </div>
  {/each}
  {#if store.habitsFor(date).length}
    <div class="habits-row"><HabitCheckins {date} /></div>
  {/if}
</section>

<style>
  .habits-row {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .quicklog {
    padding: 12px 14px;
    margin-bottom: 14px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 10px;
    margin-bottom: 6px;
  }
  .links {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px 12px;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: 13px;
  }
  .health-row {
    align-items: flex-start;
  }
  .health-row :global(.health) {
    flex: 1 1 260px;
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
</style>
