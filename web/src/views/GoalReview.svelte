<script lang="ts">
  // The periodic goals review (SPEC §6.10): where each goal stands, a note on each, and
  // the day's reflection alongside.
  import Icon from '../lib/components/Icon.svelte'
  import Reflection from '../lib/components/Reflection.svelte'
  import { longDate } from '../lib/dates'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'

  $effect(() => {
    store.loadGoalProgress()
  })
  const goals = $derived(store.goalList().filter((g) => g.status === 'active'))
  let notes = $state<Record<string, string>>({})
  let saving = $state(false)
  async function finish() {
    saving = true
    try {
      await store.submitReview(goals.map((g) => ({ goal_id: g.id, note: notes[g.id] ?? '' })))
      toast('Review saved')
      router.go('/goals')
    } catch {
      toast('Could not save the review', 'error')
    } finally {
      saving = false
    }
  }
  const pct = (id: string) => {
    const p = store.goalProgress.get(id)?.progress
    return p === null || p === undefined ? null : Math.round(p * 100)
  }
</script>

<h1>{store.me?.review_cadence === 'monthly' ? 'Monthly' : 'Weekly'} review</h1>
<p class="muted lead">How are your goals going? A line or two each is plenty. Next steps can go straight into a goal's milestones.</p>

{#each goals as g (g.id)}
  {@const p = pct(g.id)}
  <section class="card">
    <div class="top">
      <a href="/goals"><strong>{g.title}</strong></a>
      <span class="pct">{p === null ? '–' : `${p}%`}</span>
    </div>
    <div class="progress"><span style:width="{p ?? 0}%"></span></div>
    {#if g.target_date}<p class="muted small">Target: {longDate(g.target_date)}</p>{/if}
    {#each g.milestones.filter((m) => !m.done).slice(0, 3) as m (m.id)}<p class="small next">○ {m.title}</p>{/each}
    <textarea rows="2" bind:value={notes[g.id]} placeholder="Progress, blockers, next step…" aria-label="Note on {g.title}"></textarea>
  </section>
{:else}
  <p class="muted">No active goals. <a href="/goals">Add one</a>.</p>
{/each}

<section class="card">
  <h2>Today's reflection</h2>
  <Reflection date={store.today} compact />
</section>

<div class="row">
  <button class="btn primary" onclick={finish} disabled={saving}><Icon name="check" size={16} /> Finish review</button>
  <a class="btn" href="/goals">Later</a>
</div>

<style>
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .lead {
    margin: 4px 0 14px;
  }
  section {
    padding: 14px 16px;
    margin-bottom: 12px;
  }
  h2 {
    font-size: 15px;
    margin-bottom: 8px;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .top a {
    color: inherit;
  }
  .pct {
    font-weight: 700;
  }
  .progress {
    height: 6px;
    border-radius: 3px;
    background: var(--border);
    overflow: hidden;
    margin: 6px 0;
  }
  .progress span {
    display: block;
    height: 100%;
    background: var(--ok);
  }
  .small {
    font-size: 12px;
    margin: 2px 0;
  }
  .next {
    color: var(--muted);
  }
  textarea {
    width: 100%;
    margin-top: 8px;
  }
  .row {
    display: flex;
    gap: 8px;
  }
</style>
