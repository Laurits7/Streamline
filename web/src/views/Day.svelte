<script lang="ts">
  // The landing view: what to expect today (or any other day).
  import type { DayEntry } from '../lib/api/types/DayEntry'
  import type { Task } from '../lib/api/types/Task'
  import Icon from '../lib/components/Icon.svelte'
  import QuickAdd from '../lib/components/QuickAdd.svelte'
  import TaskRow from '../lib/components/TaskRow.svelte'
  import { addDays, dayLabel, fmtMinutes, longDate, nowHHMM } from '../lib/dates'
  import { router } from '../lib/router.svelte'
  import { sortable } from '../lib/sortable'
  import { store } from '../lib/store.svelte'
  import { ui } from '../lib/ui.svelte'

  let { date: dateProp = null }: { date?: string | null } = $props()
  const date = $derived(dateProp ?? store.today)
  const isToday = $derived(date === store.today)

  type Item = { entry: DayEntry; task: Task }
  const items = $derived(
    store
      .dayEntries(date)
      .map((entry) => ({ entry, task: store.tasks.get(entry.task_id) }))
      .filter((i): i is Item => !!i.task),
  )
  const open = $derived(items.filter((i) => i.task.status === 'open'))
  const scheduled = $derived(
    open.filter((i) => i.entry.start_time).sort((a, b) => (a.entry.start_time! < b.entry.start_time! ? -1 : 1)),
  )
  const flexible = $derived(open.filter((i) => !i.entry.start_time))
  const closed = $derived(items.filter((i) => i.task.status !== 'open'))
  const plannedIds = $derived(new Set(items.map((i) => i.task.id)))
  const due = $derived(
    [...store.tasks.values()]
      .filter((t) => t.status === 'open' && t.due_date && t.due_date <= date && !plannedIds.has(t.id))
      .sort((a, b) => (a.due_date! < b.due_date! ? -1 : 1)),
  )

  const doneCount = $derived(closed.filter((i) => i.task.status === 'done').length)
  const minutesLeft = $derived(open.reduce((s, i) => s + (i.entry.duration_min ?? i.task.estimate_min ?? 0), 0))

  // Current time, refreshed every 30 s, for the "now" marker.
  let now = $state(nowHHMM(store.me?.timezone ?? 'UTC'))
  $effect(() => {
    const t = setInterval(() => (now = nowHHMM(store.me?.timezone ?? 'UTC')), 30_000)
    return () => clearInterval(t)
  })
  const nowIndex = $derived(isToday ? scheduled.findIndex((i) => i.entry.start_time! >= now) : -1)
  const next = $derived(isToday ? (nowIndex >= 0 ? scheduled[nowIndex] : (flexible[0] ?? null)) : null)

  let showDone = $state(false)
  const go = (d: string) => router.go(d === store.today ? '/' : `/day/${d}`)
</script>

<header class="head">
  <div class="title">
    <h1>{dayLabel(date, store.today)}</h1>
    <p class="muted">{longDate(date)}</p>
  </div>
  <nav class="daynav" aria-label="Change day">
    <button class="icon-btn" onclick={() => go(addDays(date, -1))} aria-label="Previous day"><Icon name="left" /></button>
    {#if !isToday}<button class="btn small" onclick={() => go(store.today)}>Today</button>{/if}
    <button class="icon-btn" onclick={() => go(addDays(date, 1))} aria-label="Next day"><Icon name="right" /></button>
  </nav>
</header>

{#if items.length}
  <div class="progress" aria-label="{doneCount} of {items.length} done">
    <div class="bar"><div style:width="{(doneCount / items.length) * 100}%"></div></div>
    <span class="muted">{doneCount}/{items.length} done{minutesLeft ? ` · ${fmtMinutes(minutesLeft)} left` : ''}</span>
  </div>
{/if}

{#if next}
  <button class="next card" onclick={() => (ui.editing = next.task.id)}>
    <span class="label">{next.entry.start_time ? `Next · ${next.entry.start_time}` : 'Up next'}</span>
    <span class="next-title">{next.task.title}</span>
  </button>
{/if}

<QuickAdd placeholder={isToday ? 'Add a task for today…' : `Add a task for ${dayLabel(date, store.today).toLowerCase()}…`} onadd={(title) => store.createTask({ title, day: date })} />

{#if scheduled.length}
  <h2 class="section-title"><Icon name="clock" size={14} /> Scheduled</h2>
  <div class="card list">
    {#each scheduled as i, idx (i.entry.id)}
      {#if idx === nowIndex}<div class="now"><span>Now {now}</span></div>{/if}
      <div class:past={isToday && nowIndex !== -1 && idx < nowIndex || isToday && nowIndex === -1}>
        <TaskRow task={i.task} entry={i.entry} showProject />
      </div>
    {/each}
  </div>
{/if}

<h2 class="section-title">
  <Icon name="list" size={14} /> Plan
  <button class="btn small pull" onclick={() => (ui.pullFor = date)}><Icon name="plus" size={14} /> Pull in tasks</button>
</h2>
{#if flexible.length}
  <div
    class="card list"
    use:sortable={{ onMove: (id, index) => store.reorderEntry(flexible.map((i) => i.entry), id, index) }}>
    {#each flexible as i (i.entry.id)}
      <TaskRow task={i.task} entry={i.entry} showProject draggable />
    {/each}
  </div>
{:else}
  <div class="card empty">
    {#if items.length}All planned tasks are done.{:else}Nothing planned yet.{/if}
    <br /><button class="btn primary small" style="margin-top:12px" onclick={() => (ui.pullFor = date)}>Pull in tasks</button>
  </div>
{/if}

{#if due.length}
  <h2 class="section-title"><Icon name="calendar" size={14} /> Due {isToday ? '& overdue' : ''}</h2>
  <div class="card list">
    {#each due as t (t.id)}
      <TaskRow task={t} showProject planButton planDate={date} />
    {/each}
  </div>
{/if}

{#if closed.length}
  <button class="section-title toggle" onclick={() => (showDone = !showDone)} aria-expanded={showDone}>
    <Icon name={showDone ? 'left' : 'right'} size={14} /> Done & closed ({closed.length})
  </button>
  {#if showDone}
    <div class="card list">
      {#each closed as i (i.entry.id)}
        <TaskRow task={i.task} entry={i.entry} showProject />
      {/each}
    </div>
  {/if}
{/if}

<style>
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 16px;
  }
  h1 {
    font-size: 30px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .title p {
    margin: 2px 0 0;
  }
  .daynav {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
    margin-bottom: 14px;
  }
  .bar {
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 300ms ease;
  }
  .next {
    width: 100%;
    display: flex;
    flex-direction: column;
    text-align: left;
    padding: 14px 16px;
    margin-bottom: 14px;
    border-left: 4px solid var(--accent);
    box-shadow: var(--shadow);
  }
  .next .label {
    font-size: 12px;
    font-weight: 650;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .next-title {
    font-size: 17px;
    font-weight: 600;
  }
  .list {
    overflow: hidden;
  }
  .pull {
    margin-left: auto;
    text-transform: none;
    letter-spacing: 0;
  }
  .toggle {
    width: 100%;
  }
  .now {
    position: relative;
    height: 0;
    border-top: 2px solid var(--danger);
    z-index: 1;
  }
  .now span {
    position: absolute;
    right: 8px;
    top: -10px;
    font-size: 11px;
    font-weight: 700;
    background: var(--danger);
    color: #fff;
    border-radius: 999px;
    padding: 1px 8px;
  }
  .past {
    opacity: 0.6;
  }
</style>
