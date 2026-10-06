<script lang="ts">
  import { addDays, fmtMinutes, longDate, shortDate } from '../dates'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'
  import Check from './Check.svelte'
  import Icon from './Icon.svelte'
  import Sheet from './Sheet.svelte'
  import { router } from '../router.svelte'
  import { describeSeries } from '../rrule'
  import { createsCycle } from '../deps'

  let { id }: { id: string } = $props()

  const task = $derived(store.tasks.get(id))
  const entry = $derived(store.entryForTask(id))
  const projects = $derived(store.projectTree())
  const types = $derived([...store.taskTypes.values()])
  // Tasks this one could wait for: open, not itself, not already chosen, no cycles.
  const candidates = $derived.by(() => {
    const t = store.tasks.get(id)
    if (!t) return []
    return [...store.tasks.values()]
      .filter(
        (c) =>
          c.id !== id &&
          c.status === 'open' &&
          !t.depends_on.includes(c.id) &&
          !store.isUpcoming(c) &&
          !createsCycle(id, [...t.depends_on, c.id], (x) => store.tasks.get(x)?.depends_on ?? []),
      )
      .sort((a, b) => a.title.localeCompare(b.title))
  })

  let title = $state('')
  let notes = $state('')
  // Re-seed the inputs when another task is opened.
  $effect.pre(() => {
    const t = store.tasks.get(id)
    title = t?.title ?? ''
    notes = t?.notes ?? ''
  })

  const close = () => (ui.editing = null)

  function saveTitle() {
    const t = title.trim()
    if (task && t && t !== task.title) store.updateTask(id, { title: t })
    else if (task) title = task.title
  }
  function saveNotes() {
    if (task && notes !== task.notes) store.updateTask(id, { notes })
  }

  const estimates = [5, 15, 30, 60, 120]
  const levels = ['None', 'Low', 'Medium', 'High']
  const behaviorHelp: Record<string, string> = {
    carry: 'If not done, it rolls over to the next day.',
    expire: 'If not done by the end of the day, it is marked missed.',
    window: 'Can be done any time within its window.',
    deadline: 'Carries on until its due date, then shows as overdue.',
  }

  let customDate = $state('')
  function planOn(date: string) {
    if (date) store.plan(id, date)
  }
  function remove() {
    if (task && confirm(`Delete “${task.title}”?`)) {
      store.deleteTask(id)
      close()
    }
  }
</script>

{#if task}
  <Sheet title="Edit task" onclose={close}>
    {#snippet header()}
      <div class="head">
        <Check done={task.status === 'done'} onclick={() => store.toggleDone(id)} label="Toggle done" />
        <span class="muted">{task.status === 'open' ? (task.started_at ? 'In progress' : 'Open') : task.status === 'done' ? 'Done' : task.status.replace('_', ' ')}</span>
      </div>
    {/snippet}

    <input
      class="title-input"
      type="text"
      bind:value={title}
      onblur={saveTitle}
      onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()}
      aria-label="Title" />

    {#if task.series_id}
      {@const series = store.series.get(task.series_id)}
      <div class="routine-box">
        <Icon name="repeat" size={16} />
        <span>
          {#if series}Part of a routine: <strong>{describeSeries(series)}</strong>{:else}Part of a routine that has ended{/if}
          {#if task.occurrence_date}<span class="muted"> · this one is for {shortDate(task.occurrence_date, store.today)}{task.window_end ? `–${shortDate(task.window_end, store.today)}` : ''}</span>{/if}
        </span>
        <div class="acts">
          {#if task.status === 'open'}<button class="btn small" onclick={() => store.updateTask(id, { status: 'skipped' })}>Skip this time</button>{/if}
          {#if series}<button class="btn small" onclick={() => { ui.routine = series.id; ui.editing = null }}>Edit routine</button>{/if}
        </div>
      </div>
      {#if task.status === 'open'}
        <label class="move-day">
          <span>Clashes with something? Move this one to</span>
          <input type="date" min={store.today} value={task.due_date ?? ''} onchange={(e) => { const v = (e.currentTarget as HTMLInputElement).value; if (v) store.moveOccurrence(id, v) }} />
        </label>
      {/if}
      <p class="muted help">Changes below apply to this occurrence only.</p>
    {/if}

    {#if task.workflow_instance_id}
      {@const run = store.workflowRun(task.workflow_instance_id)}
      <div class="chain">
        <h3>Step {task.workflow_step} of {task.workflow_steps}</h3>
        <ol>
          {#each run as r (r.id)}
            <li class:current={r.id === id} class:done={r.status !== 'open'}>
              <span class="dot">{r.status === 'done' ? '✓' : r.workflow_step}</span>
              <button onclick={() => (ui.editing = r.id)}>{r.title}</button>
              {#if r.status === 'open' && r.blocked}<span class="muted">waiting</span>{/if}
            </li>
          {/each}
        </ol>
      </div>
    {/if}

    {#if task.status === 'open'}
      <div class="quick">
        <button class="btn primary small" onclick={() => { store.focus('start', id); ui.editing = null; router.go('/focus') }}>
          <Icon name="target" size={14} /> Focus on this
        </button>
        <button class="chip" class:on={!!task.started_at} aria-pressed={!!task.started_at} onclick={() => store.setInProgress(id, !task.started_at)}>
          In progress
        </button>
        {#if task.actual_min}<span class="muted spent"><Icon name="clock" size={12} /> {fmtMinutes(task.actual_min)} spent</span>{/if}
      </div>
    {/if}

    <section>
      <h3><Icon name="sun" size={14} /> Day plan</h3>
      {#if entry}
        <p class="planned">
          Planned for <strong>{shortDate(entry.date, store.today)}</strong>
          <span class="muted">· {longDate(entry.date)}</span>
        </p>
        <div class="grid2">
          <label>
            <span>Time</span>
            <input
              type="time"
              value={entry.start_time ?? ''}
              onchange={(e) => store.updateEntry(entry.id, { start_time: (e.currentTarget as HTMLInputElement).value || null })} />
          </label>
          <label>
            <span>Duration</span>
            <select
              value={entry.duration_min ?? ''}
              onchange={(e) => {
                const v = (e.currentTarget as HTMLSelectElement).value
                store.updateEntry(entry.id, { duration_min: v ? Number(v) : null })
              }}>
              <option value="">Use estimate</option>
              {#each [15, 30, 45, 60, 90, 120, 180, 240] as m (m)}<option value={m}>{fmtMinutes(m)}</option>{/each}
            </select>
          </label>
        </div>
      {/if}
      <div class="chips">
        <button class="chip" class:on={entry?.date === store.today} onclick={() => planOn(store.today)}>Today</button>
        <button class="chip" class:on={entry?.date === addDays(store.today, 1)} onclick={() => planOn(addDays(store.today, 1))}>Tomorrow</button>
        <label class="chip date-chip">
          <Icon name="calendar" size={14} />
          <input type="date" bind:value={customDate} onchange={() => planOn(customDate)} aria-label="Plan on date" />
        </label>
        {#if entry}<button class="chip" onclick={() => store.unplan(entry.id)}>Not planned</button>{/if}
      </div>
    </section>

    <section>
      <h3>Project</h3>
      <select
        value={task.project_id ?? ''}
        onchange={(e) => store.moveToProject(id, (e.currentTarget as HTMLSelectElement).value || null)}>
        <option value="">Inbox</option>
        {#each projects as { project: p, depth } (p.id)}<option value={p.id}>{'\u00a0\u00a0\u00a0'.repeat(depth)}{p.name}</option>{/each}
      </select>
      <div class="also">
        <span class="muted">Also in:</span>
        {#each task.also_project_ids as pid (pid)}
          <span class="chip on also-chip">
            {store.projectPath(pid)}
            <button aria-label="Remove from {store.projectPath(pid)}" onclick={() => store.setAlsoProjects(id, task.also_project_ids.filter((x) => x !== pid))}><Icon name="x" size={12} /></button>
          </span>
        {/each}
        <select
          class="add-also"
          value=""
          aria-label="Also list in another project"
          onchange={(e) => {
            const v = (e.currentTarget as HTMLSelectElement).value
            if (v) store.setAlsoProjects(id, [...task.also_project_ids, v])
            ;(e.currentTarget as HTMLSelectElement).value = ''
          }}>
          <option value="">+ Add project</option>
          {#each projects.filter(({ project: p }) => p.id !== task.project_id && !task.also_project_ids.includes(p.id)) as { project: p, depth } (p.id)}
            <option value={p.id}>{'\u00a0\u00a0\u00a0'.repeat(depth)}{p.name}</option>
          {/each}
        </select>
      </div>
    </section>

    {#if store.places.size}
      <section>
        <h3><Icon name="pin" size={14} /> Place</h3>
        <select value={task.place_id ?? ''} onchange={(e) => store.updateTask(id, { place_id: (e.currentTarget as HTMLSelectElement).value || null })}>
          <option value="">Anywhere</option>
          {#each store.placeList() as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
      </section>
    {/if}

    <section>
      <h3>Waits for</h3>
      {#if task.depends_on.length}
        <ul class="deps">
          {#each task.depends_on as did (did)}
            {@const d = store.tasks.get(did)}
            <li>
              <span class:done={d?.status !== 'open'}>{d?.status === 'open' ? '⏳' : '✓'} {d?.title ?? '(deleted)'}</span>
              <button class="icon-btn" aria-label="Remove prerequisite" onclick={() => store.setPrerequisites(id, task.depends_on.filter((x) => x !== did))}><Icon name="x" size={14} /></button>
            </li>
          {/each}
        </ul>
      {/if}
      <select
        value=""
        aria-label="Add a prerequisite"
        onchange={(e) => {
          const v = (e.currentTarget as HTMLSelectElement).value
          if (v) store.setPrerequisites(id, [...task.depends_on, v])
          ;(e.currentTarget as HTMLSelectElement).value = ''
        }}>
        <option value="">+ Must be done after…</option>
        {#each candidates as c (c.id)}<option value={c.id}>{c.title}</option>{/each}
      </select>
      {#if task.depends_on.length}
        <label class="wait">
          <span>Then wait</span>
          <select value={task.wait_min ?? ''} onchange={(e) => store.updateTask(id, { wait_min: (e.currentTarget as HTMLSelectElement).value ? Number((e.currentTarget as HTMLSelectElement).value) : null })}>
            <option value="">no extra time</option>
            {#each [15, 30, 45, 60, 90, 120, 180, 240] as m (m)}<option value={m}>{fmtMinutes(m)}</option>{/each}
          </select>
          <span class="muted">(e.g. while a machine runs)</span>
        </label>
      {/if}
    </section>

    <section>
      <h3>Due date</h3>
      <div class="row">
        <input
          type="date"
          value={task.due_date ?? ''}
          onchange={(e) => store.updateTask(id, { due_date: (e.currentTarget as HTMLInputElement).value || null })} />
        {#if task.due_date}<button class="btn small" onclick={() => store.updateTask(id, { due_date: null })}>Clear</button>{/if}
      </div>
    </section>

    <section>
      <h3>Estimate</h3>
      <div class="chips">
        {#each estimates as m (m)}
          <button class="chip" class:on={task.estimate_min === m} onclick={() => store.updateTask(id, { estimate_min: task.estimate_min === m ? null : m })}>{fmtMinutes(m)}</button>
        {/each}
        <input
          class="custom-min"
          type="number"
          min="0"
          max="1440"
          placeholder="min"
          aria-label="Custom estimate in minutes"
          value={task.estimate_min !== null && !estimates.includes(task.estimate_min) ? task.estimate_min : ''}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLInputElement).value
            store.updateTask(id, { estimate_min: v ? Math.min(1440, Math.max(0, Number(v))) : null })
          }} />
      </div>
    </section>

    <section>
      <h3>Difficulty</h3>
      <div class="chips">
        {#each ['Easy', 'Medium', 'Hard'] as label, i (label)}
          <button class="chip" class:on={task.difficulty === i + 1} onclick={() => store.updateTask(id, { difficulty: task.difficulty === i + 1 ? null : i + 1 })}>{label}</button>
        {/each}
      </div>
    </section>

    <section class="grid2">
      <div>
        <h3>Importance</h3>
        <div class="chips">
          {#each levels as label, i (label)}
            <button class="chip" class:on={(task.importance ?? 0) === i} onclick={() => store.updateTask(id, { importance: i || null })}>{label}</button>
          {/each}
        </div>
      </div>
      <div>
        <h3>Urgency</h3>
        <div class="chips">
          {#each levels as label, i (label)}
            <button class="chip" class:on={(task.urgency ?? 0) === i} onclick={() => store.updateTask(id, { urgency: i || null })}>{label}</button>
          {/each}
        </div>
      </div>
    </section>

    <section>
      <h3>If not done by day end</h3>
      <div class="chips">
        {#each types as t (t.id)}
          <button class="chip" class:on={task.task_type_id === t.id} onclick={() => store.updateTask(id, { task_type_id: t.id })}>{t.name}</button>
        {/each}
      </div>
      <p class="muted help">{behaviorHelp[store.taskType(task.task_type_id)?.day_end_behavior ?? 'carry']}</p>
    </section>

    <section>
      <h3>Notes</h3>
      <textarea bind:value={notes} onblur={saveNotes} rows="5" placeholder="Details, links, checklist…"></textarea>
    </section>

    <footer>
      {#if task.status === 'open'}
        <button class="btn" onclick={() => store.updateTask(id, { status: 'wont_do' })}>Won't do</button>
      {:else if task.status !== 'done'}
        <button class="btn" onclick={() => store.updateTask(id, { status: 'open' })}>Reopen</button>
      {/if}
      <button class="btn danger" onclick={remove}><Icon name="trash" size={16} /> Delete</button>
    </footer>
  </Sheet>
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    text-transform: capitalize;
    font-size: 14px;
  }
  .title-input {
    font-size: 19px !important;
    font-weight: 600;
    border-color: transparent !important;
    padding-left: 0 !important;
    background: transparent !important;
  }
  .title-input:focus {
    border-color: var(--border) !important;
    padding-left: 12px !important;
  }
  section {
    margin-top: 20px;
  }
  .move-day {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    margin-top: 8px;
  }
  .move-day input {
    width: auto !important;
    padding: 4px 8px !important;
  }
  .chain {
    margin-top: 14px;
  }
  .chain ol {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .chain li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
    font-size: 14px;
  }
  .chain .dot {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 700;
    background: var(--surface-3);
  }
  .chain .done .dot {
    background: var(--ok);
    color: #fff;
  }
  .chain .current .dot {
    background: var(--accent);
    color: var(--accent-text);
  }
  .chain .done button {
    color: var(--muted);
    text-decoration: line-through;
  }
  .chain .current button {
    font-weight: 700;
  }
  .deps {
    list-style: none;
    padding: 0;
    margin: 0 0 8px;
  }
  .deps li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .deps .done {
    color: var(--muted);
    text-decoration: line-through;
  }
  .wait {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    font-size: 13px;
  }
  .wait select {
    width: auto !important;
  }
  .also {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    font-size: 13px;
  }
  .also-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding-right: 6px;
  }
  .also-chip button {
    display: grid;
    color: inherit;
  }
  .add-also {
    width: auto !important;
    padding: 4px 8px !important;
    font-size: 13px;
  }
  .routine-box {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 14px;
  }
  .routine-box > span {
    flex: 1 1 200px;
    color: var(--text);
  }
  .routine-box .acts {
    display: flex;
    gap: 6px;
  }
  .quick {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .spent {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 13px;
  }
  h3 {
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
    margin-bottom: 8px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    margin-bottom: 10px;
  }
  label span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .planned {
    margin: 0 0 10px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .date-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding-right: 4px;
  }
  .date-chip input {
    border: 0;
    background: transparent;
    padding: 4px;
    width: auto;
    font-size: 13px;
  }
  .custom-min {
    width: 80px !important;
    padding: 4px 10px !important;
    border-radius: 999px !important;
    font-size: 13px;
  }
  .help {
    font-size: 13px;
    margin: 8px 2px 0;
  }
  footer {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin-top: 28px;
  }
</style>
