<script lang="ts">
  // The planning wizard (SPEC §6.2d): Review → Look ahead → Pick → Arrange → Confirm.
  // Progress is saved as a draft, so it resumes at the same step on any device.
  import { untrack } from 'svelte'
  import { api } from '../lib/api/client'
  import type { DayEntry } from '../lib/api/types/DayEntry'
  import type { Task } from '../lib/api/types/Task'
  import Icon from '../lib/components/Icon.svelte'
  import LoadBar from '../lib/components/LoadBar.svelte'
  import DayLog from '../lib/components/DayLog.svelte'
  import QuickAdd from '../lib/components/QuickAdd.svelte'
  import Reflection from '../lib/components/Reflection.svelte'
  import TaskRow from '../lib/components/TaskRow.svelte'
  import Timeline from '../lib/components/Timeline.svelte'
  import { addDays, dayLabel, fmtMinutes, longDate, nowHHMM } from '../lib/dates'
  import { dropList, type DragItem } from '../lib/dnd.svelte'
  import { keyAt } from '../lib/order'
  import { dayLoad } from '../lib/planning'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'
  import type { SuggestedPlan } from '../lib/api/types/SuggestedPlan'

  let { date }: { date: string } = $props()

  // "Suggest times" (Arrange step): the server proposes, the user picks what to keep.
  let suggestion = $state<SuggestedPlan | null>(null)
  let picked = $state<Record<string, boolean>>({})
  let suggesting = $state(false)
  async function suggestTimes() {
    suggesting = true
    try {
      suggestion = await store.suggestPlan(date)
      picked = Object.fromEntries(suggestion.suggestions.map((x) => [x.task_id, true]))
    } catch {
      suggestion = null
    } finally {
      suggesting = false
    }
  }
  function applySuggestion() {
    for (const x of suggestion?.suggestions ?? []) {
      if (!picked[x.task_id]) continue
      const entry = store.dayEntries(date).find((e) => e.task_id === x.task_id)
      if (!entry) continue
      const estimated = store.tasks.get(x.task_id)?.estimate_min != null
      store.updateEntry(entry.id, { start_time: x.start_time, ...(estimated ? {} : { duration_min: x.duration_min }) })
    }
    suggestion = null
  }
  const endOf = (start: string, min: number) => {
    const m = Number(start.slice(0, 2)) * 60 + Number(start.slice(3, 5)) + min
    return `${String(Math.floor(m / 60) % 24).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`
  }
  function reasonText(r: { code: string; days?: number }, blockId: string | null): string {
    const block = blockId ? store.timeBlocks.get(blockId)?.title : null
    switch (r.code) {
      case 'overdue': return 'overdue'
      case 'due_today': return 'due today'
      case 'due_tomorrow': return 'due tomorrow'
      case 'due_soon': return `due in ${r.days} days`
      case 'expires_today': return 'only doable today'
      case 'urgent': return 'urgent'
      case 'important': return 'important'
      case 'hard_in_hard_block': return `hard → ${block ?? 'hard block'}`
      case 'easy_in_easy_block': return `easy → ${block ?? 'easy block'}`
      case 'earliest_free_time': return 'first free time'
      case 'estimate_assumed': return 'no estimate, 30 min assumed'
      default: return r.code
    }
  }

  const STEPS = ['Review', 'Look ahead', 'Pick', 'Arrange', 'Confirm']
  // Start where planning was left off (read once; the view is re-created per date).
  let step = $state(untrack(() => Math.min(store.planFor(date)?.step ?? 0, STEPS.length - 1)))
  let confirmed = $state(false)

  function go(n: number) {
    step = Math.max(0, Math.min(STEPS.length - 1, n))
    store.setPlan(date, 'draft', step)
    window.scrollTo(0, 0)
  }
  // Make sure routine occurrences exist for days after tomorrow.
  $effect(() => {
    if (date > addDays(store.today, 1)) api.get(`/days/${date}`).catch(() => {})
  })
  // Opening the wizard counts as starting to plan.
  $effect(() => {
    if (!store.planFor(date)) store.setPlan(date, 'draft', step)
  })

  const label = $derived(dayLabel(date, store.today).toLowerCase())
  const title = $derived(
    date === store.today ? 'Plan today' : date === addDays(store.today, 1) ? 'Plan tomorrow' : `Plan ${longDate(date)}`,
  )
  const reviewDate = $derived(addDays(date, -1))
  const reviewLabel = $derived(dayLabel(reviewDate, store.today).toLowerCase())

  type Item = { entry: DayEntry; task: Task }
  const itemsOn = (d: string): Item[] =>
    store
      .dayEntries(d)
      .map((entry) => ({ entry, task: store.tasks.get(entry.task_id) }))
      .filter((i): i is Item => !!i.task)

  // Review
  const review = $derived(itemsOn(reviewDate))
  const done = $derived(review.filter((i) => i.task.status === 'done'))
  const missed = $derived(review.filter((i) => i.task.status === 'missed'))
  const stillOpen = $derived(review.filter((i) => i.task.status === 'open'))
  // Tasks moved forward from the review list in this session, so they don't just vanish.
  let movedForward = $state<string[]>([])
  function moveForward(taskId: string, title: string) {
    store.plan(taskId, date)
    movedForward = [...movedForward, title]
  }
  const carried = $derived(reviewDate < store.today ? itemsOn(date).filter((i) => i.task.carry_count > 0 && i.task.status === 'open') : [])

  // Target day
  const target = $derived(itemsOn(date))
  const timed = $derived(
    target.filter((i) => i.entry.start_time).sort((a, b) => (a.entry.start_time! < b.entry.start_time! ? -1 : 1)),
  )
  const flexible = $derived(target.filter((i) => !i.entry.start_time && i.task.status === 'open'))
  const plannedIds = $derived(new Set(target.map((i) => i.task.id)))
  const due = $derived(
    [...store.tasks.values()]
      .filter(
        (t) =>
          t.status === 'open' && t.due_date && t.due_date <= date && !plannedIds.has(t.id) && !store.isUpcoming(t, date) && !store.inIdea(t),
      )
      .sort((a, b) => (a.due_date! < b.due_date! ? -1 : 1)),
  )

  // Pick
  let q = $state('')
  const groups = $derived.by(() => {
    const needle = q.trim().toLowerCase()
    const tasks = store.readyStack(date).filter((t) => !needle || t.title.toLowerCase().includes(needle))
    const out: { name: string; color: string | null; tasks: Task[] }[] = []
    const inbox = tasks.filter((t) => !t.project_id)
    if (inbox.length) out.push({ name: 'Inbox', color: null, tasks: inbox })
    for (const { project: p } of store.projectTree()) {
      const ts = tasks.filter((t) => t.project_id === p.id)
      if (ts.length) out.push({ name: store.projectPath(p.id), color: p.color, tasks: ts })
    }
    return out
  })
  const diffLabel = ['', 'Easy', 'Medium', 'Hard']

  // Arrange
  const mq = window.matchMedia('(min-width: 1100px)')
  let wide = $state(mq.matches)
  $effect(() => {
    const on = () => (wide = mq.matches)
    mq.addEventListener('change', on)
    return () => mq.removeEventListener('change', on)
  })
  function dropOnPlan(item: DragItem, index: number) {
    if (item.kind !== 'task') return
    const others = flexible.filter((i) => i.task.id !== item.taskId).map((i) => i.entry.position)
    store.plan(item.taskId, date, { position: keyAt(others, index), startTime: null })
  }

  // Confirm
  const load = $derived(
    dayLoad(store.me!, store.dayEntries(date), (id) => store.tasks.get(id), date === store.today ? nowHHMM(store.me!.timezone) : undefined, store.dayBusy(date)),
  )
  const isPlanned = $derived(store.isPlanned(date))
  function confirm() {
    store.setPlan(date, 'planned', STEPS.length - 1)
    confirmed = true
  }
  const close = () => router.go(date === store.today ? '/' : `/day/${date}`)
</script>

<div class="wizard">
  <header class="top">
    <button class="icon-btn" onclick={close} aria-label="Close planning"><Icon name="x" /></button>
    <div class="titles">
      <h1>{title}</h1>
      <p class="muted">{longDate(date)}{isPlanned ? ' · planned ✓' : ''}</p>
    </div>
  </header>

  <ol class="steps" aria-label="Planning steps">
    {#each STEPS as s, i (s)}
      <li>
        <button class:active={i === step} class:past={i < step} aria-current={i === step ? 'step' : undefined} onclick={() => go(i)}>
          <span class="num">{i < step ? '✓' : i + 1}</span><span class="name">{s}</span>
        </button>
      </li>
    {/each}
  </ol>

  {#if step === 0}
    <section>
      <h2>How did {reviewLabel} go?</h2>
      {#if movedForward.length}
        <p class="moved"><Icon name="right" size={14} /> Moved to {label}: {movedForward.join(', ')}</p>
      {/if}
      {#if !review.length && !carried.length}
        <p class="empty card">Nothing was planned for {reviewLabel}.</p>
      {:else}
        <p class="stats">
          <span class="ok">{done.length} done</span> · <span class:bad={missed.length}>{missed.length} missed</span> ·
          <span>{stillOpen.length} still open</span>
        </p>
        {#if stillOpen.length}
          <h3>Still open</h3>
          <p class="muted hint">
            Decide now, or leave them: at the end of the day, “Carry on” tasks move to the next day by themselves and “Expires” tasks count as missed.
          </p>
          <div class="card list">
            {#each stillOpen as i (i.entry.id)}
              <div class="review-row">
                <span class="t">{i.task.title}</span>
                <div class="acts">
                  <button class="btn small" onclick={() => store.updateTask(i.task.id, { status: 'done' })}><Icon name="check" size={14} /> Done</button>
                  {#if reviewDate !== date}
                    <button class="btn small" onclick={() => moveForward(i.task.id, i.task.title)}>→ {label}</button>
                  {/if}
                  <button class="btn small" onclick={() => store.unplan(i.entry.id)} title="Back to its project">Unplan</button>
                  <button class="btn small" onclick={() => store.updateTask(i.task.id, { status: 'wont_do' })}>Won't do</button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
        {#if carried.length}
          <h3>Carried over to {label}</h3>
          <div class="card list">
            {#each carried as i (i.entry.id)}<TaskRow task={i.task} entry={i.entry} showProject />{/each}
          </div>
        {/if}
        {#if missed.length}
          <h3>Missed</h3>
          <div class="card list">{#each missed as i (i.entry.id)}<TaskRow task={i.task} entry={i.entry} showProject />{/each}</div>
        {/if}
        {#if reviewDate <= store.today}<DayLog date={reviewDate} open={false} />{/if}
        {#if done.length}
          <h3>Done</h3>
          <ul class="done-list">{#each done as i (i.entry.id)}<li><Icon name="check" size={14} /> {i.task.title}</li>{/each}</ul>
        {/if}
      {/if}
      <h3>Reflect on {reviewLabel}</h3>
      <div class="card reflect">
        {#key reviewDate}<Reflection date={reviewDate} compact />{/key}
      </div>
    </section>
  {:else if step === 1}
    <section>
      <h2>What's already fixed for {label}</h2>
      {#if timed.length}
        <h3><Icon name="clock" size={14} /> Scheduled</h3>
        <div class="card list">
          {#each timed as i (i.entry.id)}<TaskRow task={i.task} entry={i.entry} showProject />{/each}
        </div>
      {/if}
      {#if due.length}
        <h3><Icon name="calendar" size={14} /> Due {date === store.today ? 'or overdue' : 'by then'}</h3>
        <div class="card list">
          {#each due as t (t.id)}<TaskRow task={t} showProject planButton planDate={date} />{/each}
        </div>
      {/if}
      {#if flexible.length}
        <h3>Already on the plan</h3>
        <ul class="done-list">{#each flexible as i (i.entry.id)}<li>{i.task.title}</li>{/each}</ul>
      {/if}
      {#if !timed.length && !due.length && !flexible.length}
        <p class="empty card">Nothing is fixed yet: a blank slate.</p>
      {/if}
    </section>
  {:else if step === 2}
    <section>
      <h2>Pick what to do {label}</h2>
      <div class="sticky"><LoadBar {date} /></div>
      {#if flexible.length || timed.length}
        <h3>On the plan ({flexible.length + timed.filter((i) => i.task.status === 'open').length})</h3>
        <div class="chips">
          {#each [...timed.filter((i) => i.task.status === 'open'), ...flexible] as i (i.entry.id)}
            <span class="chip on picked">
              {i.entry.start_time ? `${i.entry.start_time} ` : ''}{i.task.title}
              <button aria-label="Remove {i.task.title}" onclick={() => store.unplan(i.entry.id)}><Icon name="x" size={12} /></button>
            </span>
          {/each}
        </div>
      {/if}
      <h3>Ready to pick</h3>
      <QuickAdd placeholder="Add a new task for {label}…" onadd={(title) => store.createTask({ title, day: date })} />
      <input class="search" type="text" bind:value={q} placeholder="Search tasks…" aria-label="Search tasks" />
      {#each groups as g (g.name)}
        <h4><i style:background={g.color ?? 'var(--faint)'}></i>{g.name}</h4>
        <div class="card list">
          {#each g.tasks as t (t.id)}
            {@const other = store.entryForTask(t.id)}
            <button class="pick" onclick={() => store.plan(t.id, date)}>
              <span class="add"><Icon name="plus" size={16} /></span>
              <span class="t">{t.title}</span>
              <span class="meta">
                {#if other}<span>{dayLabel(other.date, store.today)}</span>{/if}
                {#if t.difficulty}<span>{diffLabel[t.difficulty]}</span>{/if}
                {#if t.estimate_min}<span><Icon name="clock" size={12} /> {fmtMinutes(t.estimate_min)}</span>{/if}
                {#if t.due_date}<span class:overdue={t.due_date <= date}>due {dayLabel(t.due_date, store.today)}</span>{/if}
              </span>
            </button>
          {/each}
        </div>
      {:else}
        <p class="empty">{q ? 'No matching tasks.' : 'Everything open is already planned.'}</p>
      {/each}
    </section>
  {:else if step === 3}
    <section>
      <h2>Arrange {label}</h2>
      <p class="muted hint">Drag to set the order. Drop tasks on the timeline to give them a time, or let Streamline suggest times.</p>
      <div class="suggest">
        <button class="btn" onclick={suggestTimes} disabled={suggesting || !flexible.length}>
          <Icon name="zap" size={16} /> {suggesting ? 'Thinking…' : 'Suggest times'}
        </button>
        {#if !store.blocksOn(date).length}<span class="muted small">Tip: time blocks (Blocks… on the timeline) let it match hard and easy work to the right time.</span>{/if}
      </div>
      {#if suggestion}
        <div class="card suggestion">
          {#if suggestion.suggestions.length}
            <ul>
              {#each suggestion.suggestions as x (x.task_id)}
                <li>
                  <label>
                    <input type="checkbox" bind:checked={picked[x.task_id]} />
                    <span class="when">{x.start_time}–{endOf(x.start_time, x.duration_min)}</span>
                    <span class="what">{store.tasks.get(x.task_id)?.title}</span>
                  </label>
                  <span class="why muted">{x.reasons.map((r) => reasonText(r, x.block_id)).join(' · ')}</span>
                </li>
              {/each}
            </ul>
          {/if}
          {#each suggestion.unplaced as u (u.task_id)}
            <p class="unplaced">
              <strong>{store.tasks.get(u.task_id)?.title}</strong>:
              {u.reason.code === 'blocked' ? 'waiting for a prerequisite' : `no free ${fmtMinutes(u.reason.minutes)} left`}
            </p>
          {/each}
          <div class="row">
            {#if suggestion.suggestions.length}<button class="btn primary" onclick={applySuggestion}>Use selected</button>{/if}
            <button class="btn" onclick={() => (suggestion = null)}>Dismiss</button>
          </div>
        </div>
      {/if}
      <div class="sticky"><LoadBar {date} /></div>
      <div class="arrange" class:wide>
        <div>
          <h3>Order</h3>
          <div
            class="card list"
            use:dropList={{
              accepts: (it) => it.kind === 'task',
              drop: dropOnPlan,
              keyMove: (id, index) => store.reorderEntry(flexible.map((i) => i.entry), id, index),
            }}>
            {#each flexible as i (i.entry.id)}
              <TaskRow task={i.task} entry={i.entry} showProject handle />
            {:else}
              <p class="empty">No unscheduled tasks. Drop timeline blocks here to remove their time.</p>
            {/each}
          </div>
        </div>
        <div>
          <h3>Timeline</h3>
          <Timeline {date} items={timed} now={null} />
        </div>
      </div>
    </section>
  {:else}
    <section>
      {#if confirmed}
        <div class="done-state">
          <div class="big-check"><Icon name="check" size={40} /></div>
          <h2>{dayLabel(date, store.today)} is planned</h2>
          <p class="muted">{target.filter((i) => i.task.status === 'open').length} tasks · {fmtMinutes(load.planned)} planned · {timed.length} scheduled</p>
          <div class="row">
            <a class="btn primary" href={date === store.today ? '/' : `/day/${date}`}>Open {label}</a>
            {#if date !== store.today}<a class="btn" href="/">Back to today</a>{/if}
          </div>
        </div>
      {:else}
        <h2>Ready?</h2>
        <div class="summary card">
          <div><strong>{target.filter((i) => i.task.status === 'open').length}</strong><span>tasks</span></div>
          <div><strong>{timed.filter((i) => i.task.status === 'open').length}</strong><span>scheduled</span></div>
          <div><strong>{fmtMinutes(load.planned)}</strong><span>planned</span></div>
          <div><strong>{fmtMinutes(load.free)}</strong><span>free</span></div>
        </div>
        {#if load.planned > load.free}
          <p class="warn">
            That's {fmtMinutes(load.planned - load.free)} more than your free time. You can still go ahead, or go back and move something.
          </p>
        {/if}
        <button class="btn primary big" onclick={confirm}><Icon name="check" size={18} /> {isPlanned ? `Save the plan for ${label}` : `Mark ${label} as planned`}</button>
        {#if isPlanned}
          <button class="link" onclick={() => { store.clearPlan(date); close() }}>Mark as not planned</button>
        {/if}
      {/if}
    </section>
  {/if}

  {#if !confirmed}
    <footer class="nav">
      <button class="btn" onclick={() => (step === 0 ? close() : go(step - 1))}>{step === 0 ? 'Later' : 'Back'}</button>
      {#if step < STEPS.length - 1}
        <button class="btn primary" onclick={() => go(step + 1)}>Next: {STEPS[step + 1]} <Icon name="right" size={16} /></button>
      {/if}
    </footer>
  {/if}
</div>

<style>
  .reflect {
    padding: 12px 14px;
  }
  .suggest {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 12px;
    align-items: center;
    margin-bottom: 12px;
  }
  .suggestion {
    padding: 12px 14px;
    margin-bottom: 14px;
  }
  .suggestion ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .suggestion li {
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .suggestion label {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .suggestion .when {
    font-variant-numeric: tabular-nums;
    font-size: 13px;
    color: var(--muted);
    min-width: 92px;
  }
  .suggestion .what {
    font-weight: 600;
  }
  .suggestion .why {
    display: block;
    font-size: 12px;
    margin-left: 26px;
  }
  .unplaced {
    font-size: 13px;
    color: var(--warn);
    margin: 8px 0 0;
  }
  .small {
    font-size: 12px;
  }
  .wizard {
    max-width: 900px;
    padding-bottom: 80px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }
  h1 {
    font-size: 26px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .titles p {
    margin: 0;
  }
  .steps {
    list-style: none;
    padding: 0;
    margin: 0 0 18px;
    display: flex;
    gap: 4px;
    overflow-x: auto;
  }
  .steps li {
    flex: 1;
    min-width: 0;
  }
  .steps button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px;
    border-radius: 10px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
    border-bottom: 3px solid var(--surface-3);
  }
  .steps button.past {
    border-color: var(--accent);
  }
  .steps button.active {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .num {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface-3);
    font-size: 12px;
  }
  .active .num,
  .past .num {
    background: var(--accent);
    color: var(--accent-text);
  }
  .name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @media (max-width: 600px) {
    .steps .name {
      display: none;
    }
    .steps button.active .name {
      display: inline;
    }
    .steps li:has(.active) {
      flex: 3;
    }
  }
  h2 {
    font-size: 20px;
    margin: 4px 0 12px;
  }
  h3 {
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
    margin: 22px 2px 8px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  h4 {
    font-size: 13px;
    margin: 16px 2px 6px;
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }
  h4 i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .hint {
    font-size: 13px;
    margin: -4px 2px 10px;
  }
  .moved {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--accent);
    font-size: 14px;
    font-weight: 550;
  }
  .stats {
    font-size: 15px;
  }
  .stats .ok {
    color: var(--ok);
    font-weight: 600;
  }
  .stats .bad {
    color: var(--danger);
    font-weight: 600;
  }
  .list {
    overflow: hidden;
  }
  .review-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .review-row:last-child {
    border-bottom: 0;
  }
  .review-row .t {
    flex: 1 1 200px;
    font-weight: 550;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .done-list {
    list-style: none;
    padding: 0 4px;
    margin: 0;
    color: var(--muted);
  }
  .done-list li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }
  .sticky {
    position: sticky;
    top: 8px;
    z-index: 5;
    margin-bottom: 8px;
  }
  .picked {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding-right: 6px;
  }
  .picked button {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    color: inherit;
  }
  .picked button:hover {
    background: var(--surface-3);
  }
  .search {
    margin-top: 10px;
  }
  .pick {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 10px;
    padding: 12px;
    text-align: left;
    border-bottom: 1px solid var(--border);
  }
  .pick:last-child {
    border-bottom: 0;
  }
  .pick:hover {
    background: var(--surface-2);
  }
  .pick .add {
    color: var(--accent);
    display: grid;
  }
  .pick .t {
    flex: 1;
    min-width: 0;
  }
  .meta {
    display: flex;
    gap: 10px;
    font-size: 12px;
    color: var(--muted);
  }
  .meta span {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .overdue {
    color: var(--danger);
  }
  .arrange {
    display: grid;
    gap: 8px;
  }
  .arrange.wide {
    grid-template-columns: minmax(0, 1fr) 360px;
    gap: 28px;
    align-items: start;
  }
  .summary {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    padding: 16px 8px;
    text-align: center;
    margin-bottom: 14px;
  }
  .summary div {
    display: flex;
    flex-direction: column;
  }
  .summary strong {
    font-size: 22px;
  }
  .summary span {
    font-size: 12px;
    color: var(--muted);
  }
  .warn {
    color: var(--warn);
    font-weight: 600;
    background: var(--warn-soft);
    padding: 10px 12px;
    border-radius: var(--radius-sm);
  }
  .big {
    width: 100%;
    min-height: 52px;
    font-size: 16px;
    margin-top: 6px;
  }
  .link {
    display: block;
    margin: 14px auto 0;
    color: var(--muted);
    text-decoration: underline;
    font-size: 13px;
  }
  .done-state {
    text-align: center;
    padding: 32px 0;
  }
  .big-check {
    width: 72px;
    height: 72px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    display: grid;
    place-items: center;
    margin: 0 auto 16px;
    animation: pop 300ms ease-out;
  }
  @keyframes pop {
    from {
      transform: scale(0.6);
      opacity: 0;
    }
  }
  .row {
    display: flex;
    gap: 8px;
    justify-content: center;
    margin-top: 16px;
  }
  .nav {
    position: fixed;
    left: 0;
    right: 0;
    bottom: calc(var(--nav-h) + env(safe-area-inset-bottom));
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 10px 16px;
    background: color-mix(in srgb, var(--bg) 90%, transparent);
    backdrop-filter: blur(12px);
    border-top: 1px solid var(--border);
    z-index: 30;
  }
  @media (min-width: 900px) {
    .nav {
      left: var(--sidebar-w);
      bottom: 0;
      padding: 12px 32px;
    }
  }
</style>
