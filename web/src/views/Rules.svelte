<script lang="ts">
  // Rules for yourself (D-112): "eat less junk", kept from a day on, for good or until a
  // date. No daily question and no progress; a rule can sit under a goal, and the weekly
  // review lists the active ones as a reminder.
  import type { Goal } from '../lib/api/types/Goal'
  import Icon from '../lib/components/Icon.svelte'
  import { longDate } from '../lib/dates'
  import { goalTree, parentOptions } from '../lib/goals'
  import { store } from '../lib/store.svelte'

  const rules = $derived(store.goalList().filter((g) => g.kind === 'rule'))
  const current = $derived(rules.filter((r) => r.status === 'active' || r.status === 'paused'))
  const ended = $derived(rules.filter((r) => r.status === 'achieved' || r.status === 'dropped'))
  // Goals a rule can go under: open outcome goals (nested as in the tree).
  const goalChoices = $derived(
    goalTree(parentOptions(store.goalList().filter((g) => g.kind === 'outcome' && (g.status === 'active' || g.status === 'paused')), null)),
  )
  let newTitle = $state('')
  let newParent = $state('')
  let open = $state<string | null>(null)
  let showEnded = $state(false)
  const STATUS = { active: 'Active', paused: 'Paused', achieved: 'Kept', dropped: 'Dropped' } as const

  function add(e: Event) {
    e.preventDefault()
    if (!newTitle.trim()) return
    store.createGoal(newTitle.trim(), null, 'rule', newParent || null)
    newTitle = ''
  }
  /** Done with it: kept until today (or its end date, if that's already past). */
  const keep = (r: Goal) =>
    store.updateGoal(r.id, { status: 'achieved', target_date: r.target_date && r.target_date <= store.today ? r.target_date : store.today })
  const period = (r: Goal) =>
    `${r.start_date ? `since ${longDate(r.start_date)}` : ''}${r.target_date ? `${r.start_date ? ', ' : ''}until ${longDate(r.target_date)}` : r.start_date ? ', for good' : 'for good'}`
</script>

<header class="head">
  <h1>Rules</h1>
</header>
<p class="muted lead">
  Rules for yourself, like “Eat less junk” or “No screens after 22:00”: kept from a day on, for good or until a date. Nothing to answer each day; the
  weekly review shows them as a reminder, and on the <a href="/goals">Goals</a> page they sit under the goal they serve.
</p>

<form class="row new" onsubmit={add}>
  <input type="text" bind:value={newTitle} placeholder="A rule, e.g. Eat less junk" aria-label="New rule" />
  {#if goalChoices.length}
    <select bind:value={newParent} aria-label="Under a goal">
      <option value="">No goal</option>
      {#each goalChoices as { goal, depth } (goal.id)}<option value={goal.id}>{'   '.repeat(depth)}{goal.title}</option>{/each}
    </select>
  {/if}
  <button class="btn primary" type="submit">Add rule</button>
</form>

{#snippet card(r: Goal)}
  {@const parent = r.parent_id ? store.goals.get(r.parent_id) : undefined}
  <article class="card rule" class:open={open === r.id}>
    <button class="summary" onclick={() => (open = open === r.id ? null : r.id)} aria-expanded={open === r.id}>
      <span class="title">{r.title}{#if r.status !== 'active'}<span class="tag">{STATUS[r.status]}</span>{/if}</span>
      <span class="muted small">
        {period(r)}{#if parent}{' · '}<Icon name="flag" size={11} /> {parent.title}{/if}{#if r.owner_group_id}{` · ${store.groups.get(r.owner_group_id)?.name ?? 'shared'}`}{/if}
      </span>
    </button>
    {#if open === r.id}
      <div class="detail">
        <label><span>Rule</span><input type="text" value={r.title} onchange={(e) => store.updateGoal(r.id, { title: (e.currentTarget as HTMLInputElement).value })} /></label>
        <label><span>Why, and what counts</span><textarea rows="3" value={r.description} onchange={(e) => store.updateGoal(r.id, { description: (e.currentTarget as HTMLTextAreaElement).value })}></textarea></label>
        <div class="grid3">
          <label><span>From</span><input type="date" value={r.start_date ?? ''} onchange={(e) => store.updateGoal(r.id, { start_date: (e.currentTarget as HTMLInputElement).value || null })} /></label>
          <label><span>Until (empty: for good)</span><input type="date" value={r.target_date ?? ''} onchange={(e) => store.updateGoal(r.id, { target_date: (e.currentTarget as HTMLInputElement).value || null })} /></label>
          <label>
            <span>Status</span>
            <select value={r.status} onchange={(e) => store.updateGoal(r.id, { status: (e.currentTarget as HTMLSelectElement).value as Goal['status'] })}>
              {#each Object.entries(STATUS) as [v, label] (v)}<option value={v}>{label}</option>{/each}
            </select>
          </label>
          <label>
            <span>Under the goal</span>
            <select value={r.parent_id ?? ''} onchange={(e) => store.updateGoal(r.id, { parent_id: (e.currentTarget as HTMLSelectElement).value || null })}>
              <option value="">No goal</option>
              {#each goalChoices as { goal, depth } (goal.id)}<option value={goal.id}>{'   '.repeat(depth)}{goal.title}</option>{/each}
              {#if parent && !goalChoices.some((c) => c.goal.id === parent.id)}<option value={parent.id}>{parent.title}</option>{/if}
            </select>
          </label>
          {#if !r.parent_id && store.myGroups().length}
            <label>
              <span>Shared with</span>
              <select value={r.owner_group_id ?? ''} onchange={(e) => store.updateGoal(r.id, { owner_group_id: (e.currentTarget as HTMLSelectElement).value || null })}>
                <option value="">Only me</option>
                {#each store.myGroups() as gr (gr.id)}<option value={gr.id}>{gr.name}</option>{/each}
              </select>
            </label>
          {/if}
        </div>
        <div class="row actions">
          {#if r.status === 'active' || r.status === 'paused'}
            <button class="btn small" onclick={() => keep(r)}><Icon name="check" size={14} /> Kept it, end now</button>
            <button class="btn small" onclick={() => store.updateGoal(r.id, { status: 'dropped' })}>Drop</button>
          {:else}
            <button class="btn small" onclick={() => store.updateGoal(r.id, { status: 'active', target_date: null })}>Take it up again</button>
          {/if}
          <button class="btn small danger" onclick={() => confirm(`Delete “${r.title}”?`) && store.deleteGoal(r.id)}>Delete</button>
        </div>
      </div>
    {/if}
  </article>
{/snippet}

<div class="cards">{#each current as r (r.id)}{@render card(r)}{:else}<p class="muted">No rules yet. Add one above.</p>{/each}</div>
{#if ended.length}
  <button class="section-title toggle" onclick={() => (showEnded = !showEnded)} aria-expanded={showEnded}>
    <Icon name={showEnded ? 'left' : 'right'} size={14} /> Kept & dropped ({ended.length})
  </button>
  {#if showEnded}<div class="cards">{#each ended as r (r.id)}{@render card(r)}{/each}</div>{/if}
{/if}

<style>
  .head {
    margin-bottom: 6px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .lead {
    margin: 0 0 14px;
    max-width: 70ch;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .row select {
    width: auto;
  }
  .new {
    margin-bottom: 14px;
  }
  .new input {
    flex: 1 1 200px;
  }
  .cards {
    display: grid;
    gap: 10px;
    margin-bottom: 12px;
  }
  .summary {
    display: grid;
    gap: 2px;
    width: 100%;
    text-align: left;
    padding: 12px 16px;
  }
  .title {
    font-weight: 650;
    font-size: 16px;
  }
  .tag {
    margin-left: 8px;
    font-size: 11px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--muted);
  }
  .detail {
    padding: 0 16px 16px;
    border-top: 1px solid var(--border);
  }
  label {
    display: block;
    margin: 10px 0 0;
  }
  label span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 3px;
  }
  .grid3 {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 0 12px;
  }
  .actions {
    margin-top: 14px;
  }
  .small {
    font-size: 12px;
  }
  .toggle {
    width: 100%;
  }
</style>
