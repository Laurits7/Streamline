<script lang="ts">
  // Week planner (D-94): a column per day with every planned task and the day's load
  // against its free time. Tasks drag between days (keeping their time) and in from the
  // list of unplanned tasks; each day takes new tasks too. Days not set up yet preview what
  // their template will plan (D-113).
  import type { Task } from '../api/types/Task'
  import { dayLabel, fmtMinutes, nowHHMM } from '../dates'
  import { announce } from '../announce.svelte'
  import { draggable, droppable, type DragItem } from '../dnd.svelte'
  import { dayLoad } from '../planning'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'
  import { hhmm } from '../weekTemplates'
  import Check from './Check.svelte'
  import Icon from './Icon.svelte'

  let { days }: { days: string[] } = $props()
  /** The unplanned list: open when there's something in it, until toggled by hand. */
  let toggled = $state<boolean | null>(null)
  let drafts = $state<Record<string, string>>({})
  let over = $state<string | null>(null)

  const unplanned = $derived(store.readyStack(store.today).filter((t) => !store.entryForTask(t.id)))
  const showUnplanned = $derived(toggled ?? unplanned.length > 0)

  function rows(date: string) {
    return store
      .dayEntries(date)
      .map((e) => ({ entry: e, task: store.tasks.get(e.task_id) }))
      .filter((r): r is { entry: typeof r.entry; task: Task } => !!r.task && r.task.status !== 'missed' && r.task.status !== 'skipped')
      .sort((a, b) => Number(!a.entry.start_time) - Number(!b.entry.start_time) || (a.entry.start_time ?? '').localeCompare(b.entry.start_time ?? ''))
  }
  const load = (date: string) => store.dayLoadFor(date)
  function drop(item: DragItem, date: string) {
    over = null
    if (item.kind !== 'task') return
    const entry = item.entryId ? store.entries.get(item.entryId) : store.entryForTask(item.taskId)
    if (entry?.date === date) return
    const t = store.tasks.get(item.taskId)
    if (t?.status === 'done') {
      store.moveDone(item.taskId, date)
      announce(`“${t.title}” done ${dayLabel(date, store.today).toLowerCase()}`)
      return
    }
    store.plan(item.taskId, date, { startTime: entry?.start_time ?? null })
    announce(`“${t?.title}” planned for ${dayLabel(date, store.today)}`)
  }
  /** Open tasks go on today or later; a done one onto the day it was done, up to today (D-111). */
  function accepts(item: DragItem, date: string): boolean {
    if (item.kind !== 'task') return false
    return store.tasks.get(item.taskId)?.status === 'done' ? date <= store.today : date >= store.today
  }
  function add(e: Event, date: string) {
    e.preventDefault()
    const title = (drafts[date] ?? '').trim()
    if (!title) return
    store.createTask({ title, day: date })
    drafts[date] = ''
  }
</script>

<div class="planner" class:with-list={showUnplanned}>
  <aside class="unplanned card" aria-label="Unplanned tasks">
    <button class="toggle" aria-expanded={showUnplanned} onclick={() => (toggled = !showUnplanned)}>
      <Icon name="inbox" size={15} /> Unplanned <span class="muted">{unplanned.length}</span>
    </button>
    {#if showUnplanned}
      <p class="muted hint">Drag onto a day.</p>
      <ul>
        {#each unplanned.slice(0, 80) as t (t.id)}
          <li
            class="task"
            use:draggable={{ item: () => ({ kind: 'task', taskId: t.id, durationMin: t.estimate_min ?? 30 }) }}>
            <button class="title" title={t.title} onclick={() => (ui.editing = t.id)}>{t.title}</button>
            {#if t.estimate_min}<span class="est">{fmtMinutes(t.estimate_min)}</span>{/if}
          </li>
        {:else}
          <li class="muted none">Nothing waiting to be planned.</li>
        {/each}
      </ul>
    {/if}
  </aside>

  <div class="days">
    {#each days as date (date)}
      {@const l = load(date)}
      {@const past = date < store.today}
      {@const pv = store.previewOf(date)}
      <section
        class="day card"
        class:today={date === store.today}
        class:past
        class:over={over === date}
        aria-label={dayLabel(date, store.today)}
        use:droppable={{
          accepts: (it) => accepts(it, date),
          over: () => (over = date),
          leave: () => over === date && (over = null),
          drop: (it) => drop(it, date),
        }}>
        <a class="dayhead" href={date === store.today ? '/' : `/day/${date}`}>
          <strong>{dayLabel(date, store.today)}</strong>
          <span class="muted">{new Date(date + 'T00:00:00Z').toLocaleDateString(store.me?.locale || undefined, { day: 'numeric', month: 'short', timeZone: 'UTC' })}</span>
        </a>
        {#if !past}
          <div class="load" class:overbooked={l.planned > l.free} title="{fmtMinutes(l.planned)} planned of {fmtMinutes(l.free)} free">
            <div class="bar"><div style:width="{l.free > 0 ? Math.min(100, (l.planned / l.free) * 100) : l.planned ? 100 : 0}%"></div></div>
            <span>{fmtMinutes(l.planned)} / {fmtMinutes(l.free)}</span>
          </div>
        {/if}
        <ul>
          {#each rows(date) as r (r.entry.id)}
            <li
              class="task"
              class:done={r.task.status !== 'open'}
              data-task={r.task.id}
              use:draggable={{
                disabled: r.task.status !== 'open' && r.task.status !== 'done',
                item: () => ({ kind: 'task', taskId: r.task.id, entryId: r.entry.id, durationMin: r.entry.duration_min ?? r.task.estimate_min ?? 30 }),
              }}>
              <span data-nodrag class="chk" title={store.tooEarly(r.task) ? store.earlyTitle(r.task) : undefined}
                ><Check
                  done={r.task.status === 'done'}
                  disabled={store.tooEarly(r.task)}
                  title={store.tooEarly(r.task) ? store.earlyTitle(r.task) : undefined}
                  onclick={() => store.toggleDone(r.task.id, date)}
                  label="Complete {r.task.title}" /></span
              >
              <button class="title main" title={r.task.title} onclick={() => (ui.editing = r.task.id)}>
                {#if r.entry.start_time}<span class="time">{r.entry.start_time}</span>{/if}{r.task.title}
              </button>
              {#if r.task.estimate_min}<span class="est">{fmtMinutes(r.task.estimate_min)}</span>{/if}
            </li>
          {:else}
            {#if !pv}<li class="muted none">{past ? 'Nothing was planned.' : 'Free'}</li>{/if}
          {/each}
        </ul>
        {#if pv}
          <ul class="pv" aria-label="Preview of {pv.template.name}">
            <li class="pv-head" title="What “{pv.template.name}” will set up on this day (days are set up 8 days ahead)">Preview · {pv.template.name}</li>
            {#each pv.blocks as b (b.key)}
              <li class="pv-block"><span class="time">{hhmm(b.start)}</span>{b.title}</li>
              {#each b.items as it (it.id)}
                <li class="pv-row" title={it.maybe ? 'If it has a slot left that day' : undefined}>
                  {#if it.start !== null}<span class="time">{hhmm(it.start)}</span>{/if}{it.title}{it.maybe ? '?' : ''}
                </li>
              {/each}
            {/each}
          </ul>
        {/if}
        {#if !past}
          <form class="add" onsubmit={(e) => add(e, date)}>
            <Icon name="plus" size={14} />
            <input type="text" bind:value={drafts[date]} placeholder="Add" aria-label="Add a task for {dayLabel(date, store.today)}" />
          </form>
        {/if}
      </section>
    {/each}
  </div>
</div>

<style>
  .planner {
    display: grid;
    grid-template-columns: 44px 1fr;
    gap: 12px;
    align-items: start;
  }
  .planner.with-list {
    grid-template-columns: 190px 1fr;
  }
  .unplanned {
    padding: 8px;
    position: sticky;
    top: 12px;
    max-height: calc(100dvh - 140px);
    overflow-y: auto;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 650;
    font-size: 14px;
    width: 100%;
  }
  .planner:not(.with-list) .toggle {
    writing-mode: vertical-rl;
    padding: 6px 0;
  }
  .hint {
    font-size: 12px;
    margin: 4px 0 6px;
  }
  .days {
    display: grid;
    grid-template-columns: repeat(7, minmax(118px, 1fr));
    gap: 8px;
    overflow-x: auto;
    padding-bottom: 6px;
  }
  .day {
    padding: 8px;
    min-height: 220px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .day.today {
    outline: 2px solid var(--accent);
  }
  .day.past {
    background: var(--bg);
    box-shadow: none;
  }
  .day.over {
    outline: 2px dashed var(--accent);
    background: var(--accent-soft);
  }
  .dayhead {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 6px;
    color: inherit;
    font-size: 14px;
  }
  .dayhead span {
    font-size: 12px;
  }
  .load {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .load .bar {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .load .bar div {
    height: 100%;
    background: var(--accent);
  }
  .load.overbooked {
    color: var(--danger);
  }
  .load.overbooked .bar div {
    background: var(--danger);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .task {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 6px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    font-size: 13px;
    cursor: grab;
  }
  .task.done {
    opacity: 0.6;
  }
  .task.done .title {
    text-decoration: line-through;
  }
  .chk {
    display: contents;
  }
  .chk :global(.check) {
    width: 16px;
    height: 16px;
  }
  .title {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: inherit;
  }
  .time {
    font-variant-numeric: tabular-nums;
    color: var(--accent);
    font-weight: 650;
    margin-right: 4px;
  }
  .est {
    font-size: 11px;
    color: var(--muted);
    flex: none;
  }
  .pv {
    opacity: 0.75;
  }
  .pv-head {
    font-size: 11px;
    font-weight: 650;
    color: var(--accent);
  }
  .pv-block {
    font-size: 12px;
    font-weight: 650;
    padding: 2px 2px 0;
  }
  .pv-row {
    font-size: 12px;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
    border: 1px dashed var(--border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none {
    font-size: 12px;
    padding: 4px 2px;
  }
  .add {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: auto;
    color: var(--muted);
  }
  .add input {
    flex: 1;
    min-width: 0;
    font-size: 13px;
    padding: 4px 6px !important;
  }
  /* Phones: days one under the other, the unplanned list above. */
  @media (max-width: 720px) {
    .planner,
    .planner.with-list {
      grid-template-columns: 1fr;
    }
    .unplanned {
      position: static;
      max-height: 40dvh;
    }
    .planner:not(.with-list) .toggle {
      writing-mode: horizontal-tb;
    }
    .days {
      grid-template-columns: 1fr;
    }
    .day {
      min-height: 0;
    }
  }
</style>
