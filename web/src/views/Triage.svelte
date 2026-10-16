<script lang="ts">
  // Inbox triage (10.3): one item at a time, one key per decision, undo with u. The list
  // is taken when it starts; what's decided elsewhere meanwhile is passed over.
  import { untrack } from 'svelte'
  import Icon from '../lib/components/Icon.svelte'
  import WaitPicker from '../lib/components/WaitPicker.svelte'
  import { addDays, dayLabel, fmtMinutes } from '../lib/dates'
  import { typing } from '../lib/keys'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import { current, decide, decisionFor, DECISIONS, start, tally, triageQueue, undo, type Decision, type Session } from '../lib/triage'

  const fresh = () => start(triageQueue(store.tasks.values(), (id) => !!store.entryForTask(id)).map((t) => t.id))
  let session = $state<Session>(untrack(fresh))
  /** How to reverse each decision, by its place in the history. */
  let undos: (() => void)[] = []
  let panel = $state<null | 'project' | 'day' | 'waiting'>(null)
  let projectQuery = $state('')
  let day = $state(addDays(store.today, 2))
  let title = $state('')

  const id = $derived(current(session))
  const task = $derived(id ? store.tasks.get(id) : undefined)
  const remaining = $derived(session.ids.length - session.at)
  const projects = $derived(
    store
      .projectTree()
      .filter(({ project: p }) => !p.archived_at && p.name.toLowerCase().includes(projectQuery.trim().toLowerCase()))
      .slice(0, 8),
  )

  // Decided elsewhere meanwhile (completed on the phone, filed…): pass over it.
  $effect(() => {
    if (id && (!task || task.status !== 'open' || task.project_id)) untrack(() => (session = decide(session, 'skip'), undos.push(() => {})))
  })
  $effect(() => {
    title = task?.title ?? ''
    panel = null
  })

  function record(d: Decision, reverse: () => void) {
    undos.push(reverse)
    session = decide(session, d)
  }
  function plan(date: string) {
    const t = task!
    store.plan(t.id, date)
    const entry = store.entryForTask(t.id)
    record(date === store.today ? 'today' : date === addDays(store.today, 1) ? 'tomorrow' : 'day', () => entry && store.unplan(entry.id))
  }
  function choose(d: Decision) {
    const t = task
    if (!t) return
    if (d === 'done') {
      store.toggleDone(t.id)
      record(d, () => store.tasks.get(t.id)?.status === 'done' && store.toggleDone(t.id))
    } else if (d === 'today') plan(store.today)
    else if (d === 'tomorrow') plan(addDays(store.today, 1))
    else if (d === 'day' || d === 'project' || d === 'waiting') panel = panel === d ? null : d
    else if (d === 'drop') {
      store.updateTask(t.id, { status: 'wont_do' })
      record(d, () => store.updateTask(t.id, { status: 'open' }))
    } else if (d === 'skip') record(d, () => {})
    else if (d === 'focus') {
      store.focus('start', t.id)
      router.go('/focus')
    }
  }
  function fileUnder(projectId: string) {
    const t = task!
    store.updateTask(t.id, { project_id: projectId })
    record('project', () => store.updateTask(t.id, { project_id: null }))
  }
  function back() {
    const u = undo(session)
    if (!u.step) return
    undos.pop()?.()
    session = u.session
    toast('Undone')
  }
  function rename() {
    const v = title.trim()
    if (task && v && v !== task.title) store.updateTask(task.id, { title: v })
  }

  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return
      if (e.key === 'Escape' && panel) return (panel = null)
      if (typing(e.target) || document.querySelector('[role=dialog][aria-modal=true]')) return
      const d = decisionFor(e.key)
      if (d && (id || d === 'undo')) {
        e.preventDefault()
        if (d === 'undo') back()
        else choose(d)
      } else if (task && /^[1-5]$/.test(e.key)) {
        store.updateTask(task.id, { estimate_min: [5, 15, 30, 60, 120][Number(e.key) - 1] })
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  const ago = (iso: string) => {
    const days = Math.floor((Date.now() - new Date(iso).getTime()) / 864e5)
    return days <= 0 ? 'today' : days === 1 ? 'yesterday' : `${days} days ago`
  }
  const SUMMARY: Record<string, string> = { done: 'done', today: 'for today', tomorrow: 'for tomorrow', day: 'for later', project: 'filed', waiting: 'waiting', drop: "won't do", skip: 'skipped' }
</script>

<header class="head">
  <a class="icon-btn" href="/inbox" aria-label="Back to the inbox"><Icon name="left" /></a>
  <h1>Process the inbox</h1>
  {#if id}<span class="muted count">{session.at + 1} of {session.ids.length}</span>{/if}
</header>
<div class="progress" aria-hidden="true"><div style:width="{session.ids.length ? (session.at / session.ids.length) * 100 : 100}%"></div></div>

{#if task}
  <article class="card item" aria-live="polite">
    <input
      class="title"
      bind:value={title}
      aria-label="Title"
      onblur={rename}
      onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()} />
    <p class="muted meta">
      Added {ago(task.created_at)}{task.checklist.length ? ` · ${task.checklist.length} steps` : ''}{task.due_date ? ` · due ${dayLabel(task.due_date, store.today)}` : ''}
    </p>
    {#if task.notes.trim()}<p class="notes">{task.notes}</p>{/if}
    <div class="estimate" role="group" aria-label="Estimate">
      <span class="muted">Estimate</span>
      {#each [5, 15, 30, 60, 120] as m, i (m)}
        <button class="chip" class:on={task.estimate_min === m} onclick={() => store.updateTask(task.id, { estimate_min: m })} title="Key {i + 1}">{fmtMinutes(m)}</button>
      {/each}
    </div>
  </article>

  <div class="decisions">
    {#each DECISIONS as d (d.decision)}
      <button class="btn" class:primary={d.decision === 'today'} class:on={panel === d.decision} title={d.hint} onclick={() => choose(d.decision)}>
        {d.label} <kbd>{d.key === 'Delete' ? 'Del' : d.key}</kbd>
      </button>
    {/each}
    <button class="btn" disabled={!session.history.length} onclick={back} title="Undo the last decision">Undo <kbd>u</kbd></button>
  </div>

  {#if panel === 'project'}
    <div class="card panel">
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="text"
        bind:value={projectQuery}
        placeholder="Project…"
        aria-label="Find a project"
        autofocus
        onkeydown={(e) => {
          if (e.key === 'Enter' && projects[0]) fileUnder(projects[0].project.id)
          if (e.key === 'Escape') panel = null
        }} />
      <ul>
        {#each projects as p (p.project.id)}
          <li><button onclick={() => fileUnder(p.project.id)}>{store.projectPath(p.project.id)}</button></li>
        {:else}
          <li class="muted">No project matches.</li>
        {/each}
      </ul>
    </div>
  {:else if panel === 'day'}
    <form
      class="card panel row"
      onsubmit={(e) => {
        e.preventDefault()
        plan(day)
      }}>
      <input type="date" bind:value={day} min={store.today} aria-label="Day" />
      <button class="btn primary" type="submit">Plan for {dayLabel(day, store.today)}</button>
    </form>
  {:else if panel === 'waiting'}
    <div class="card panel">
      <WaitPicker taskId={task.id} ondone={() => record('waiting', () => store.stopWaiting(task.id))} />
    </div>
  {/if}
  <p class="muted keys">Keys: the letter on each button · 1–5 set the estimate · → or space skips · u undoes</p>
{:else}
  {@const t = tally(session)}
  <section class="card done-card">
    <Icon name="check" size={32} />
    {#if session.ids.length}
      <h2>Inbox processed</h2>
      <p class="muted">
        {Object.entries(t)
          .map(([k, n]) => `${n} ${SUMMARY[k] ?? k}`)
          .join(' · ')}
      </p>
      {#if remaining === 0 && session.history.length}<button class="btn" onclick={back}>Undo the last one</button>{/if}
    {:else}
      <h2>Nothing to process</h2>
      <p class="muted">Everything in the inbox is planned, waiting or filed.</p>
    {/if}
    <div class="row">
      <a class="btn" href="/inbox">Back to the inbox</a>
      <a class="btn primary" href="/week">Plan the week</a>
    </div>
  </section>
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }
  h1 {
    font-size: 24px;
    font-weight: 750;
  }
  .count {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }
  .progress {
    height: 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
    margin-bottom: 16px;
  }
  .progress div {
    height: 100%;
    background: var(--accent);
    transition: width 200ms;
  }
  .item {
    padding: 18px 20px;
    max-width: 720px;
  }
  .item .title {
    width: 100%;
    font-size: 22px;
    font-weight: 700;
    border: 0 !important;
    background: transparent !important;
    padding: 0 !important;
    box-shadow: none !important;
  }
  .meta {
    font-size: 13px;
    margin: 4px 0 0;
  }
  .notes {
    white-space: pre-wrap;
    font-size: 14px;
    margin: 10px 0 0;
  }
  .estimate {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 12px;
    font-size: 13px;
  }
  .chip {
    padding: 2px 10px;
    border-radius: 999px;
    border: 1px solid var(--border);
    font-size: 13px;
  }
  .chip.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .decisions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 14px 0;
    max-width: 720px;
  }
  .decisions .btn.on {
    outline: 2px solid var(--accent);
  }
  .decisions kbd {
    margin-left: 6px;
  }
  .panel {
    padding: 12px;
    max-width: 720px;
  }
  .panel input[type='text'] {
    width: 100%;
  }
  .panel ul {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
  }
  .panel li button {
    width: 100%;
    text-align: left;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
  }
  .panel li button:hover {
    background: var(--accent-soft);
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .keys {
    font-size: 12px;
  }
  .done-card {
    padding: 28px;
    max-width: 520px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    text-align: center;
    color: var(--accent);
  }
  .done-card h2 {
    color: var(--text);
  }
  @media (hover: none) {
    kbd,
    .keys {
      display: none;
    }
  }
</style>
