<script lang="ts">
  // The landing view: what to expect today (or any other day).
  import type { DayEntry } from '../lib/api/types/DayEntry'
  import type { Task } from '../lib/api/types/Task'
  import Icon from '../lib/components/Icon.svelte'
  import QuickAdd from '../lib/components/QuickAdd.svelte'
  import TaskRow from '../lib/components/TaskRow.svelte'
  import Timeline from '../lib/components/Timeline.svelte'
  import { addDays, dayLabel, fmtMinutes, longDate, nowHHMM } from '../lib/dates'
  import { dropList, type DragItem } from '../lib/dnd.svelte'
  import { keyAt } from '../lib/order'
  import { router } from '../lib/router.svelte'
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
  const timed = $derived(items.filter((i) => i.entry.start_time))
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

  // Current time, refreshed every 30 s.
  let now = $state(nowHHMM(store.me?.timezone ?? 'UTC'))
  $effect(() => {
    const t = setInterval(() => (now = nowHHMM(store.me?.timezone ?? 'UTC')), 30_000)
    return () => clearInterval(t)
  })
  const upcoming = $derived(
    open.filter((i) => i.entry.start_time && i.entry.start_time >= now).sort((a, b) => (a.entry.start_time! < b.entry.start_time! ? -1 : 1)),
  )
  const next = $derived(isToday ? (upcoming[0] ?? flexible[0] ?? null) : null)

  // Wide screens show the timeline beside the plan; phones show it above.
  const mq = window.matchMedia('(min-width: 1100px)')
  let wide = $state(mq.matches)
  $effect(() => {
    const on = () => (wide = mq.matches)
    mq.addEventListener('change', on)
    return () => mq.removeEventListener('change', on)
  })

  /** Drop into the plan list: plan the task here, at that position, without a time. */
  function dropOnPlan(item: DragItem, index: number) {
    if (item.kind !== 'task') return
    const others = flexible.filter((i) => i.task.id !== item.taskId).map((i) => i.entry.position)
    store.plan(item.taskId, date, { position: keyAt(others, index), startTime: null })
  }

  let showDone = $state(false)
  const go = (d: string) => router.go(d === store.today ? '/' : `/day/${d}`)
</script>

{#snippet timeline()}
  <h2 class="section-title">
    <Icon name="clock" size={14} /> Timeline
    <span class="hint">drag tasks onto a time</span>
  </h2>
  <Timeline {date} items={timed} now={isToday ? now : null} />
{/snippet}

<div class="day" class:wide>
  <div class="main-col">
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

    <QuickAdd
      placeholder={isToday ? 'Add a task for today…' : `Add a task for ${dayLabel(date, store.today).toLowerCase()}…`}
      onadd={(title) => store.createTask({ title, day: date })} />

    {#if !wide}{@render timeline()}{/if}

    <h2 class="section-title">
      <Icon name="list" size={14} /> Plan
      <button class="btn small pull" onclick={() => (ui.pullFor = date)}><Icon name="plus" size={14} /> Pull in tasks</button>
    </h2>
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
        <div class="empty">
          {#if timed.length || closed.length}Nothing left without a time.{:else}Nothing planned yet.{/if}
          Drop tasks here, or
          <button class="link" onclick={() => (ui.pullFor = date)}>pull them in</button>.
        </div>
      {/each}
    </div>

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
  </div>

  {#if wide}
    <aside class="time-col">{@render timeline()}</aside>
  {/if}
</div>

<style>
  .day.wide {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 360px;
    gap: 32px;
    align-items: start;
  }
  .time-col {
    position: sticky;
    top: 12px;
  }
  .time-col .section-title {
    margin-top: 0;
  }
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
  .hint {
    text-transform: none;
    letter-spacing: 0;
    font-weight: 400;
    color: var(--faint);
    margin-left: auto;
  }
  .toggle {
    width: 100%;
  }
  .link {
    color: var(--accent);
    font-weight: 600;
  }
  .list:global(.drop-hover) .empty {
    background: var(--accent-soft);
  }
</style>
