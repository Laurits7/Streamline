<script lang="ts">
  // Long-term goals (SPEC §6.10): progress at a glance, milestones and linked work, and
  // the periodic review.
  import type { Goal } from '../lib/api/types/Goal'
  import type { GoalReview } from '../lib/api/types/GoalReview'
  import { api } from '../lib/api/client'
  import Icon from '../lib/components/Icon.svelte'
  import { longDate } from '../lib/dates'
  import { store } from '../lib/store.svelte'
  import { ulid } from '../lib/ulid'

  $effect(() => {
    void store.goals.size
    store.loadGoalProgress()
  })
  const goals = $derived(store.goalList())
  const active = $derived(goals.filter((g) => g.status === 'active'))
  const paused = $derived(goals.filter((g) => g.status === 'paused'))
  const closed = $derived(goals.filter((g) => g.status === 'achieved' || g.status === 'dropped'))
  let showClosed = $state(false)
  let open = $state<string | null>(null)
  let newTitle = $state('')
  let history = $state<GoalReview[]>([])
  $effect(() => {
    const id = open
    history = []
    if (id) api.get<GoalReview[]>(`/goals/${id}/reviews`).then((h) => open === id && (history = h)).catch(() => {})
  })

  const pct = (g: Goal) => {
    const p = store.goalProgress.get(g.id)?.progress
    return p === null || p === undefined ? null : Math.round(p * 100)
  }
  function add(e: Event) {
    e.preventDefault()
    if (!newTitle.trim()) return
    store.createGoal(newTitle.trim())
    newTitle = ''
  }
  const setMs = (g: Goal, i: number, patch: Partial<Goal['milestones'][number]> | null) =>
    store.updateGoal(g.id, {
      milestones: patch === null ? g.milestones.filter((_, j) => j !== i) : g.milestones.map((m, j) => (j === i ? { ...m, ...patch } : m)),
    })
  let msTitle = $state('')
  function addMilestone(e: Event, g: Goal) {
    e.preventDefault()
    if (!msTitle.trim()) return
    store.updateGoal(g.id, { milestones: [...g.milestones, { id: ulid(), title: msTitle.trim(), due_date: null, done: false }] })
    msTitle = ''
  }
  // Linking tasks: search open tasks by title.
  let taskQ = $state('')
  const taskHits = (g: Goal) =>
    taskQ.trim().length < 2
      ? []
      : [...store.tasks.values()]
          .filter((t) => t.status === 'open' && !g.task_ids.includes(t.id) && t.title.toLowerCase().includes(taskQ.trim().toLowerCase()))
          .slice(0, 8)
  const projectName = (id: string) => store.projects.get(id)?.name ?? 'Project'
  const taskTitle = (id: string) => store.tasks.get(id)?.title ?? 'Task'
</script>

<header class="head">
  <h1>Goals</h1>
  <div class="row">
    {#if store.goalReviewDue()}<a class="btn primary" href="/goals/review"><Icon name="check" size={16} /> Review now</a>{:else}<a class="btn" href="/goals/review">Review</a>{/if}
    <select
      aria-label="How often to review"
      value={store.me?.review_cadence ?? 'weekly'}
      onchange={(e) => store.updateMe({ review_cadence: (e.currentTarget as HTMLSelectElement).value as 'weekly' })}>
      <option value="weekly">Review weekly</option>
      <option value="monthly">Review monthly</option>
      <option value="off">No review reminder</option>
    </select>
  </div>
</header>

{#snippet card(g: Goal)}
  {@const p = pct(g)}
  {@const prog = store.goalProgress.get(g.id)}
  <article class="card goal" class:open={open === g.id}>
    <button class="summary" onclick={() => (open = open === g.id ? null : g.id)} aria-expanded={open === g.id}>
      <span class="title">{g.title}{#if g.owner_group_id}<span class="muted small"> · {store.groups.get(g.owner_group_id)?.name ?? 'shared'}</span>{/if}</span>
      <span class="meta muted small">
        {#if g.target_date}by {longDate(g.target_date)} · {/if}{g.milestones.filter((m) => m.done).length}/{g.milestones.length} milestones{#if prog && prog.tasks}{` · ${prog.tasks_done}/${prog.tasks} tasks`}{/if}{#if g.progress_override !== null}{' · set by hand'}{/if}
      </span>
      <span class="progress" aria-label="{p ?? 0}% done"><span style:width="{p ?? 0}%"></span></span>
      <span class="pct">{p === null ? '–' : `${p}%`}</span>
    </button>
    {#if open === g.id}
      <div class="detail">
        <label><span>Title</span><input type="text" value={g.title} onchange={(e) => store.updateGoal(g.id, { title: (e.currentTarget as HTMLInputElement).value })} /></label>
        <label><span>Why it matters / what done looks like</span><textarea rows="3" value={g.description} onchange={(e) => store.updateGoal(g.id, { description: (e.currentTarget as HTMLTextAreaElement).value })}></textarea></label>
        <div class="grid3">
          <label><span>Target date</span><input type="date" value={g.target_date ?? ''} onchange={(e) => store.updateGoal(g.id, { target_date: (e.currentTarget as HTMLInputElement).value || null })} /></label>
          <label>
            <span>Status</span>
            <select value={g.status} onchange={(e) => store.updateGoal(g.id, { status: (e.currentTarget as HTMLSelectElement).value as 'active' })}>
              <option value="active">Active</option>
              <option value="paused">Paused</option>
              <option value="achieved">Achieved 🎉</option>
              <option value="dropped">Dropped</option>
            </select>
          </label>
          <label>
            <span>Shared with</span>
            <select value={g.owner_group_id ?? ''} onchange={(e) => store.updateGoal(g.id, { owner_group_id: (e.currentTarget as HTMLSelectElement).value || null })}>
              <option value="">Only me</option>
              {#each store.myGroups() as gr (gr.id)}<option value={gr.id}>{gr.name}</option>{/each}
            </select>
          </label>
        </div>

        <h3>Milestones</h3>
        {#each g.milestones as m, i (m.id)}
          <div class="ms">
            <input type="checkbox" checked={m.done} aria-label="Done: {m.title}" onchange={(e) => setMs(g, i, { done: (e.currentTarget as HTMLInputElement).checked })} />
            <input class="mtitle" class:done={m.done} type="text" value={m.title} aria-label="Milestone" onchange={(e) => setMs(g, i, { title: (e.currentTarget as HTMLInputElement).value })} />
            <input class="mdate" type="date" value={m.due_date ?? ''} aria-label="Due" onchange={(e) => setMs(g, i, { due_date: (e.currentTarget as HTMLInputElement).value || null })} />
            <button class="icon-btn" aria-label="Remove milestone" onclick={() => setMs(g, i, null)}><Icon name="x" size={14} /></button>
          </div>
        {/each}
        <form class="row" onsubmit={(e) => addMilestone(e, g)}>
          <input type="text" bind:value={msTitle} placeholder="Add a milestone" />
          <button class="btn small" type="submit">Add</button>
        </form>

        <h3>Linked work</h3>
        <p class="muted small">Progress counts done milestones plus done tasks in linked projects (with their subprojects) and linked tasks.</p>
        <div class="chips">
          {#each g.project_ids as pid (pid)}
            <span class="chip"><Icon name="folder" size={12} /> {projectName(pid)}<button aria-label="Unlink" onclick={() => store.updateGoal(g.id, { project_ids: g.project_ids.filter((x) => x !== pid) })}>×</button></span>
          {/each}
          {#each g.task_ids as tid (tid)}
            <span class="chip">{taskTitle(tid)}<button aria-label="Unlink" onclick={() => store.updateGoal(g.id, { task_ids: g.task_ids.filter((x) => x !== tid) })}>×</button></span>
          {/each}
        </div>
        <div class="row">
          <select
            value=""
            aria-label="Link a project"
            onchange={(e) => {
              const v = (e.currentTarget as HTMLSelectElement).value
              if (v) store.updateGoal(g.id, { project_ids: [...g.project_ids, v] })
              ;(e.currentTarget as HTMLSelectElement).value = ''
            }}>
            <option value="">+ Link a project…</option>
            {#each [...store.projects.values()].filter((p) => !p.archived_at && !g.project_ids.includes(p.id)) as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
          </select>
          <input type="search" bind:value={taskQ} placeholder="+ Link a task (search)" aria-label="Find a task to link" />
        </div>
        {#if taskHits(g).length}
          <ul class="hits">
            {#each taskHits(g) as t (t.id)}
              <li><button onclick={() => { store.updateGoal(g.id, { task_ids: [...g.task_ids, t.id] }); taskQ = '' }}>{t.title}</button></li>
            {/each}
          </ul>
        {/if}

        <h3>Progress</h3>
        <label class="check">
          <input type="checkbox" checked={g.progress_override !== null} onchange={(e) => store.updateGoal(g.id, { progress_override: (e.currentTarget as HTMLInputElement).checked ? (prog?.derived ?? 0) : null })} />
          Set progress by hand{#if prog?.derived !== null && prog?.derived !== undefined}{` (derived: ${Math.round(prog.derived * 100)}%)`}{/if}
        </label>
        {#if g.progress_override !== null}
          <input type="range" min="0" max="100" step="5" value={Math.round(g.progress_override * 100)} aria-label="Progress" onchange={(e) => store.updateGoal(g.id, { progress_override: Number((e.currentTarget as HTMLInputElement).value) / 100 })} />
        {/if}

        {#if history.length}
          <h3>Review notes</h3>
          <ul class="history">
            {#each history as h (h.id)}
              <li><span class="muted small">{longDate(h.date)}{h.progress !== null ? ` · ${Math.round(h.progress * 100)}%` : ''}</span> {h.note || '—'}</li>
            {/each}
          </ul>
        {/if}
        <button class="btn small danger" onclick={() => confirm(`Delete “${g.title}”? Linked work stays.`) && store.deleteGoal(g.id)}>Delete goal</button>
      </div>
    {/if}
  </article>
{/snippet}

<form class="row new" onsubmit={add}>
  <input type="text" bind:value={newTitle} placeholder="A new goal, e.g. Run a half marathon" aria-label="New goal" />
  <button class="btn primary" type="submit">Add goal</button>
</form>

{#each active as g (g.id)}{@render card(g)}{:else}<p class="muted">No active goals yet. Add one above, then link projects and tasks so everyday work counts towards it.</p>{/each}
{#if paused.length}
  <h2 class="section-title">Paused</h2>
  {#each paused as g (g.id)}{@render card(g)}{/each}
{/if}
{#if closed.length}
  <button class="section-title toggle" onclick={() => (showClosed = !showClosed)} aria-expanded={showClosed}>
    <Icon name={showClosed ? 'left' : 'right'} size={14} /> Achieved & dropped ({closed.length})
  </button>
  {#if showClosed}{#each closed as g (g.id)}{@render card(g)}{/each}{/if}
{/if}

<style>
  .head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
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
    flex: 1;
  }
  .goal {
    margin-bottom: 10px;
    overflow: hidden;
  }
  .summary {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 4px 12px;
    width: 100%;
    text-align: left;
    padding: 12px 16px;
  }
  .title {
    font-weight: 650;
    font-size: 16px;
  }
  .meta {
    grid-column: 1 / 2;
  }
  .progress {
    grid-column: 1 / 2;
    height: 8px;
    border-radius: 4px;
    background: var(--border);
    overflow: hidden;
  }
  .progress span {
    display: block;
    height: 100%;
    background: var(--ok);
    transition: width 200ms;
  }
  .pct {
    grid-column: 2;
    grid-row: 1 / 4;
    align-self: center;
    font-size: 20px;
    font-weight: 700;
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
  h3 {
    font-size: 14px;
    margin: 16px 0 6px;
  }
  .ms {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 4px;
  }
  .mtitle {
    flex: 1;
  }
  .mtitle.done {
    text-decoration: line-through;
    color: var(--muted);
  }
  .mdate {
    width: auto;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 13px;
    padding: 2px 4px 2px 10px;
    border-radius: 999px;
    background: var(--accent-soft);
  }
  .chip button {
    padding: 0 6px;
    color: var(--muted);
  }
  .hits {
    list-style: none;
    padding: 0;
    margin: 4px 0 0;
  }
  .hits button {
    padding: 4px 6px;
    color: var(--accent);
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  input[type='range'] {
    width: 100%;
  }
  .history {
    list-style: none;
    padding: 0;
    margin: 0 0 12px;
    font-size: 14px;
  }
  .history li {
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .small {
    font-size: 12px;
  }
  .toggle {
    width: 100%;
  }
</style>
