<script lang="ts">
  // The weekly review (10.4): last week in numbers, loose ends, waiting items, projects with
  // no next step and ideas, goal notes, then a look at next week. Finishing records the
  // review (the goals review is part of it), so the Today banner knows it's done.
  import { untrack } from 'svelte'
  import Icon from '../lib/components/Icon.svelte'
  import ProjectMark from '../lib/components/ProjectMark.svelte'
  import { addDays, dayLabel, fmtMinutes, longDate, nowHHMM, weekOf } from '../lib/dates'
  import { dayLoad } from '../lib/planning'
  import { looseEnds, projectsWithoutNextStep, weekStats, type LooseEnd } from '../lib/review'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import { ui } from '../lib/ui.svelte'
  import { localParts } from '../lib/waiting'

  const STEPS = ['The week', 'Loose ends', 'Waiting', 'Projects', 'Goals', 'Plan ahead'] as const
  let step = $state(0)
  /** Items dealt with during this review (they leave the lists). */
  let handled = $state<Set<string>>(new Set())
  let nextStep = $state<Record<string, string>>({})
  let notes = $state<Record<string, string>>({})
  let saving = $state(false)

  $effect(() => {
    store.loadGoalProgress()
  })

  const ws = $derived(store.me?.week_start ?? 1)
  const thisWeek = $derived(weekOf(store.today, ws))
  const lastWeek = $derived(addDays(thisWeek, -7))
  // Reviewing at the end of the week looks back at this week; at its start, at the last one.
  const reviewed = $derived(store.today >= addDays(thisWeek, 4) ? thisWeek : lastWeek)
  /** The week being planned: the one after the reviewed week (may have started already). */
  const nextWeek = $derived(addDays(reviewed, 7))
  const started = $derived(nextWeek <= store.today)
  /** Where "later" goes: that week's first day, or tomorrow when it has started (never the past). */
  const later = $derived(started ? addDays(store.today, 1) : nextWeek)
  const tz = $derived(store.me?.timezone ?? 'UTC')

  const stats = $derived(
    weekStats([...store.tasks.values()], [...store.entries.values()], [...store.focusSessions.values()], reviewed, addDays(reviewed, 7), (iso) => localParts(iso, tz).date),
  )
  // Taken once when the review opens, so acting on an item doesn't reshuffle the list.
  const ends: LooseEnd[] = untrack(() => looseEnds([...store.tasks.values()], (id) => !!store.entryForTask(id), store.today, Date.now()))
  const waiting = $derived(store.waitingTasks().filter((t) => !handled.has(t.id)))
  const bare = $derived(projectsWithoutNextStep([...store.projects.values()], [...store.tasks.values()], (id) => store.subtree(id)).filter((p) => !handled.has(p.id)))
  const ideas = $derived([...store.projects.values()].filter((p) => p.status === 'idea' && !p.archived_at && !p.deleted_at && !p.parent_id && !handled.has(p.id)))
  const goals = $derived(store.goalList().filter((g) => g.status === 'active'))
  const openEnds = $derived(ends.filter((l) => !handled.has(l.task.id) && store.tasks.get(l.task.id)?.status === 'open'))
  const counts = $derived([null, openEnds.length, waiting.length, bare.length + ideas.length, goals.length, null])

  const done = (id: string) => (handled = new Set([...handled, id]))
  const WHY = { overdue: 'Overdue', carried: 'Carried over again and again', stale: 'Untouched for three weeks' }
  const pct = (id: string) => {
    const p = store.goalProgress.get(id)?.progress
    return p === null || p === undefined ? null : Math.round(p * 100)
  }
  function addNext(e: Event, projectId: string) {
    e.preventDefault()
    const title = (nextStep[projectId] ?? '').trim()
    if (!title) return
    store.createTask({ title, project_id: projectId })
    nextStep[projectId] = ''
    done(projectId)
  }
  function load(date: string) {
    if (!store.me) return { planned: 0, free: 0 }
    return dayLoad(store.me, store.dayEntries(date), (id) => store.tasks.get(id), date === store.today ? nowHHMM(tz) : undefined, store.dayBusy(date))
  }
  async function finish() {
    saving = true
    try {
      await store.submitReview(goals.map((g) => ({ goal_id: g.id, note: notes[g.id] ?? '' })))
      toast('Weekly review done')
      router.go('/')
    } catch {
      toast('Could not save the review', 'error')
    } finally {
      saving = false
    }
  }
</script>

<header class="head">
  <h1>{store.me?.review_cadence === 'monthly' ? 'Monthly' : 'Weekly'} review</h1>
  <p class="muted">{longDate(reviewed)} – {longDate(addDays(reviewed, 6))}</p>
</header>

<nav class="steps" aria-label="Review steps">
  {#each STEPS as s, i (s)}
    <button class:on={i === step} class:past={i < step} aria-current={i === step ? 'step' : undefined} onclick={() => (step = i)}>
      <span class="n">{i + 1}</span>{s}{#if counts[i]}<span class="badge">{counts[i]}</span>{/if}
    </button>
  {/each}
</nav>

{#if step === 0}
  <section class="card">
    <div class="stats">
      <div><strong>{stats.done}</strong><span>tasks done</span></div>
      <div><strong>{stats.planned ? Math.round((stats.plannedDone / stats.planned) * 100) : 0}%</strong><span>of {stats.planned} planned done</span></div>
      <div><strong>{fmtMinutes(stats.focusMinutes)}</strong><span>focused</span></div>
      <div><strong>{stats.missed}</strong><span>missed</span></div>
    </div>
    {#if stats.topProjects.length}
      <h2>Most done in</h2>
      <ul class="plain">
        {#each stats.topProjects as tp (tp.projectId)}
          {@const p = tp.projectId ? store.projects.get(tp.projectId) : null}
          <li>{#if p}<ProjectMark project={p} size={10} /> {store.projectPath(p.id)}{:else}Inbox{/if} <span class="muted">· {tp.done}</span></li>
        {/each}
      </ul>
    {/if}
    <p class="muted small">More in <a href="/trends">Trends</a> and the <a href="/summary">summary</a>.</p>
  </section>
{:else if step === 1}
  <section class="card">
    <p class="muted lead">Decide on what's been lying around: plan it, or let it go.</p>
    {#each openEnds as l (l.task.id)}
      <div class="line">
        <button class="what" onclick={() => (ui.editing = l.task.id)}>
          <strong>{l.task.title}</strong>
          <span class="muted small">{WHY[l.why]}{l.task.due_date ? ` · due ${dayLabel(l.task.due_date, store.today)}` : ''}{l.task.project_id ? ` · ${store.projectPath(l.task.project_id)}` : ''}</span>
        </button>
        <span class="acts">
          <button class="btn small" onclick={() => (store.plan(l.task.id, store.today), done(l.task.id))}>Today</button>
          <button class="btn small" onclick={() => (store.plan(l.task.id, later), done(l.task.id))}>{started ? 'Tomorrow' : 'Next week'}</button>
          <button class="btn small" onclick={() => (store.updateTask(l.task.id, { status: 'wont_do' }), done(l.task.id))}>Won't do</button>
          <button class="btn small ghost" onclick={() => done(l.task.id)}>Keep</button>
        </span>
      </div>
    {:else}
      <p class="muted">Nothing overdue, stuck or forgotten.</p>
    {/each}
  </section>
{:else if step === 2}
  <section class="card">
    <p class="muted lead">Things you're waiting for: chase them, or note they've arrived.</p>
    {#each waiting as t (t.id)}
      <div class="line">
        <button class="what" onclick={() => (ui.editing = t.id)}>
          <strong>{t.title}</strong>
          <span class="muted small">{t.waiting_note ? `For ${t.waiting_note}` : 'Waiting'}{t.waiting_since ? ` · since ${dayLabel(localParts(t.waiting_since, tz).date, store.today)}` : ''}</span>
        </button>
        <span class="acts">
          <button class="btn small" onclick={() => (store.stopWaiting(t.id), done(t.id))}>Arrived</button>
          <button class="btn small" onclick={() => (store.toggleDone(t.id), done(t.id))}>Done</button>
          <button class="btn small ghost" onclick={() => done(t.id)}>Keep waiting</button>
        </span>
      </div>
    {:else}
      <p class="muted">You're not waiting on anything.</p>
    {/each}
  </section>
{:else if step === 3}
  <section class="card">
    <h2>Projects with no next step</h2>
    {#each bare as p (p.id)}
      <div class="line">
        <a class="what" href="/projects/{p.id}"><strong><ProjectMark project={p} size={10} /> {p.name}</strong></a>
        <form class="acts" onsubmit={(e) => addNext(e, p.id)}>
          <input type="text" bind:value={nextStep[p.id]} placeholder="Next step…" aria-label="Next step for {p.name}" />
          <button class="btn small" type="submit">Add</button>
          <button class="btn small" type="button" onclick={() => (store.updateProject(p.id, { archived: true }), done(p.id))}>Archive</button>
        </form>
      </div>
    {:else}
      <p class="muted">Every project has something to do next.</p>
    {/each}
    {#if ideas.length}
      <h2>Ideas</h2>
      {#each ideas as p (p.id)}
        <div class="line">
          <a class="what" href="/projects/{p.id}"><strong>{p.name}</strong>{#if p.description}<span class="muted small">{p.description.slice(0, 90)}</span>{/if}</a>
          <span class="acts">
            <button class="btn small" onclick={() => (store.updateProject(p.id, { status: 'active' }), done(p.id))}>Start it</button>
            <button class="btn small" onclick={() => (store.updateProject(p.id, { archived: true }), done(p.id))}>Drop</button>
            <button class="btn small ghost" onclick={() => done(p.id)}>Later</button>
          </span>
        </div>
      {/each}
    {/if}
  </section>
{:else if step === 4}
  {#each goals as g (g.id)}
    {@const p = pct(g.id)}
    <section class="card goal">
      <div class="top"><a href="/goals"><strong>{g.title}</strong></a><span>{p === null ? '–' : `${p}%`}</span></div>
      <div class="progress"><span style:width="{p ?? 0}%"></span></div>
      {#each g.milestones.filter((m) => !m.done).slice(0, 3) as m (m.id)}<p class="small">○ {m.title}</p>{/each}
      <textarea rows="2" bind:value={notes[g.id]} placeholder="Progress, blockers, next step…" aria-label="Note on {g.title}"></textarea>
    </section>
  {:else}
    <section class="card"><p class="muted">No active goals. <a href="/goals">Add one</a> if there's something bigger you're working towards.</p></section>
  {/each}
{:else}
  <section class="card">
    <p class="muted lead">How {started ? 'this' : 'next'} week looks so far. Plan it in the week planner, then finish.</p>
    <div class="days">
      {#each Array.from({ length: 7 }, (_, i) => addDays(nextWeek, i)) as d (d)}
        {@const l = load(d)}
        <div class="day" class:over={l.planned > l.free}>
          <strong>{dayLabel(d, store.today)}</strong>
          <span>{store.dayEntries(d).length} planned</span>
          <span class="muted">{fmtMinutes(l.planned)} / {fmtMinutes(l.free)}</span>
        </div>
      {/each}
    </div>
    <a class="btn" href="/week/{nextWeek}"><Icon name="columns" size={16} /> Open the week planner</a>
  </section>
{/if}

<div class="nav">
  <button class="btn" disabled={step === 0} onclick={() => (step -= 1)}><Icon name="left" size={16} /> Back</button>
  {#if step < STEPS.length - 1}
    <button class="btn primary" onclick={() => (step += 1)}>Next: {STEPS[step + 1]} <Icon name="right" size={16} /></button>
  {:else}
    <button class="btn primary" disabled={saving} onclick={finish}><Icon name="check" size={16} /> Finish the review</button>
  {/if}
</div>

<style>
  .head h1 {
    font-size: 28px;
    font-weight: 750;
  }
  .head p {
    margin: 2px 0 12px;
    font-size: 14px;
  }
  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 12px;
  }
  .steps button {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border-radius: 999px;
    border: 1px solid var(--border);
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }
  .steps button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .steps button.past {
    color: var(--text);
  }
  .n {
    font-variant-numeric: tabular-nums;
    opacity: 0.7;
  }
  .badge {
    background: var(--warn-soft);
    color: var(--warn);
    border-radius: 999px;
    padding: 0 6px;
    font-size: 11px;
  }
  .card {
    padding: 16px 18px;
    margin-bottom: 12px;
    max-width: 860px;
  }
  .lead {
    margin: 0 0 10px;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 12px;
  }
  .stats div {
    display: flex;
    flex-direction: column;
  }
  .stats strong {
    font-size: 28px;
    font-weight: 750;
    font-variant-numeric: tabular-nums;
  }
  .stats span {
    color: var(--muted);
    font-size: 13px;
  }
  h2 {
    font-size: 15px;
    margin: 16px 0 6px;
  }
  .plain {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .plain li {
    padding: 3px 0;
  }
  .small {
    font-size: 13px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .what {
    flex: 1;
    min-width: 200px;
    display: flex;
    flex-direction: column;
    text-align: left;
    color: inherit;
  }
  .acts {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: center;
  }
  .acts input {
    width: 180px;
  }
  .ghost {
    background: transparent;
  }
  .goal .top {
    display: flex;
    justify-content: space-between;
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
    background: var(--accent);
  }
  .goal textarea {
    width: 100%;
    margin-top: 6px;
  }
  .days {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(100px, 1fr));
    gap: 8px;
    margin-bottom: 12px;
  }
  .day {
    display: flex;
    flex-direction: column;
    padding: 8px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    font-size: 13px;
  }
  .day.over {
    border-color: var(--danger);
  }
  .nav {
    display: flex;
    justify-content: space-between;
    max-width: 860px;
  }
</style>
