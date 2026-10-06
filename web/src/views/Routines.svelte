<script lang="ts">
  // All routines with their schedule, streak and next date (SPEC §6.3).
  import { api } from '../lib/api/client'
  import type { SeriesStats } from '../lib/api/types/SeriesStats'
  import Icon from '../lib/components/Icon.svelte'
  import { dayLabel } from '../lib/dates'
  import { describeSeries } from '../lib/rrule'
  import { store } from '../lib/store.svelte'
  import { ui } from '../lib/ui.svelte'

  const active = $derived(store.activeSeries())
  const groups = $derived([
    { title: 'At a fixed time', items: active.filter((s) => s.mode === 'anchored') },
    { title: 'N times a week or month', items: active.filter((s) => s.mode === 'flexible') },
    { title: 'On a schedule', items: active.filter((s) => s.mode === 'repeat') },
  ])

  // Streaks and progress are computed by the server; refresh when tasks change.
  let stats = $state<Record<string, SeriesStats>>({})

  // A ready-made example (SPEC §6.3b): laundry with variants per kind of load.
  function addLaundry() {
    const step = (id: string, title: string, estimate: number | null, wait: number | null) => ({
      id,
      title,
      estimate_min: estimate,
      wait_min: wait,
      difficulty: 1,
    })
    store.saveWorkflow(null, {
      name: 'Laundry',
      steps: [step('wash', 'Wash', 5, 90), step('dry', 'Dry', 5, 60), step('iron', 'Iron', 20, null), step('fold', 'Fold and put away', 10, null)],
      variants: [
        { id: 'colours', name: 'Colours', skip: ['iron'] },
        { id: 'whites', name: 'Whites', skip: [] },
        { id: 'delicates', name: 'Delicates', skip: ['dry', 'iron'] },
        { id: 'towels', name: 'Towels', skip: ['iron'] },
        { id: 'bedding', name: 'Bedding', skip: [] },
      ],
    })
  }
  let timer: ReturnType<typeof setTimeout> | undefined
  $effect(() => {
    // Re-run when any task changes (completions move streaks).
    void store.tasks.size
    for (const t of store.tasks.values()) void t.status
    clearTimeout(timer)
    timer = setTimeout(async () => {
      try {
        const list = await api.get<SeriesStats[]>('/series/stats')
        stats = Object.fromEntries(list.map((s) => [s.series_id, s]))
      } catch {
        /* offline */
      }
    }, 300)
    return () => clearTimeout(timer)
  })
</script>

<header class="head">
  <div>
    <h1>Routines</h1>
    <p class="muted">Recurring tasks and habits. Each occurrence is a normal task you can plan, move or skip.</p>
  </div>
  <button class="btn primary" onclick={() => (ui.routine = 'new')}><Icon name="plus" size={16} /> New routine</button>
</header>

{#if !active.length}
  <div class="empty card">
    <p>No routines yet.</p>
    <p class="muted">Try “Take vitamins every morning at 8:00”, “Laundry twice a week” or “Pay rent on the 1st”.</p>
  </div>
{/if}

{#each groups.filter((g) => g.items.length) as g (g.title)}
  <h2 class="section-title">{g.title}</h2>
  <div class="card list">
    {#each g.items as s (s.id)}
      {@const st = stats[s.id]}
      {@const project = s.project_id ? store.projects.get(s.project_id) : null}
      <button class="item" onclick={() => (ui.routine = s.id)}>
        <span class="icon"><Icon name="repeat" size={18} /></span>
        <span class="main">
          <span class="title">{s.title}</span>
          <span class="meta">
            {describeSeries(s)}
            {#if project}· {project.name}{/if}
            {#if st?.next_date && s.mode !== 'flexible'}· next {dayLabel(st.next_date, store.today).toLowerCase()}{/if}
            {#if s.mode === 'flexible' && st?.window_total}· {st.window_done} of {st.window_total} this {s.window}{/if}
            {#if s.mode !== 'flexible' && st && st.week_total > 1}· {st.week_done} of {st.week_total} this week{/if}
          </span>
        </span>
        {#if st?.streak}<span class="streak" title="Streak">🔥 {st.streak}</span>{/if}
      </button>
    {/each}
  </div>
{/each}

<h2 class="section-title">Multi-step chores</h2>
<p class="muted intro">Chores done in steps, like laundry: wash → dry → fold. Each step appears when the one before is done.</p>
<div class="card list">
  {#each [...store.workflows.values()].sort((a, b) => a.name.localeCompare(b.name)) as w (w.id)}
    <div class="item wf">
      <button class="main" onclick={() => (ui.workflow = w.id)}>
        <span class="title">{w.name}</span>
        <span class="meta">{w.steps.map((s) => s.title).join(' → ')}{w.variants.length ? ` · ${w.variants.length} variants` : ''}</span>
      </button>
      <button class="btn small primary" onclick={() => (ui.startWorkflow = w.id)}><Icon name="play" size={14} /> Start</button>
    </div>
  {:else}
    <div class="item"><span class="main muted">None yet.</span>
      <button class="btn small" onclick={addLaundry}>Add example: Laundry</button>
    </div>
  {/each}
</div>
<button class="btn small new-wf" onclick={() => (ui.workflow = 'new')}><Icon name="plus" size={14} /> New multi-step chore</button>

<style>
  .intro {
    font-size: 13px;
    margin: -4px 4px 8px;
  }
  .wf .main {
    text-align: left;
  }
  .new-wf {
    margin-top: 8px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
    margin-bottom: 16px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .head p {
    margin: 4px 0 0;
    font-size: 14px;
  }
  .list {
    overflow: hidden;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 12px 14px;
    text-align: left;
    border-bottom: 1px solid var(--border);
  }
  .item:last-child {
    border-bottom: 0;
  }
  .item:hover {
    background: var(--surface-2);
  }
  .icon {
    color: var(--accent);
    display: grid;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .title {
    font-weight: 600;
  }
  .meta {
    font-size: 13px;
    color: var(--muted);
  }
  .streak {
    font-weight: 700;
    font-size: 14px;
  }
  .empty p {
    margin: 4px 0;
  }
</style>
