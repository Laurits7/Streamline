<script lang="ts">
  // The landing view: what to expect today (or any other day).
  import type { DayEntry } from '../lib/api/types/DayEntry'
  import type { Task } from '../lib/api/types/Task'
  import Icon from '../lib/components/Icon.svelte'
  import PlanBanner from '../lib/components/PlanBanner.svelte'
  import DayLog from '../lib/components/DayLog.svelte'
  import QuickLog from '../lib/components/QuickLog.svelte'
  import { dayLoad } from '../lib/planning'
  import QuickAdd from '../lib/components/QuickAdd.svelte'
  import TaskRow from '../lib/components/TaskRow.svelte'
  import Timeline from '../lib/components/Timeline.svelte'
  import { addDays, dayLabel, fmtMinutes, longDate, nowHHMM } from '../lib/dates'
  import { announce } from '../lib/announce.svelte'
  import { api } from '../lib/api/client'
  import { dropList, type DragItem } from '../lib/dnd.svelte'
  import { keyAt } from '../lib/order'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import { ui } from '../lib/ui.svelte'

  let { date: dateProp = null }: { date?: string | null } = $props()
  const date = $derived(dateProp ?? store.today)
  const isToday = $derived(date === store.today)

  type Item = { entry: DayEntry; task: Task }
  const items = $derived(
    store
      .dayEntries(date)
      .map((entry) => ({ entry, task: store.tasks.get(entry.task_id) }))
      .filter((i): i is Item => !!i.task)
      // A shared task someone else finished (e.g. the group's dog walk) leaves my day.
      .filter((i) => !(i.task.owner_group_id && i.task.status === 'done' && i.task.completed_by && i.task.completed_by !== store.me?.id)),
  )
  const open = $derived(items.filter((i) => i.task.status === 'open'))
  const timed = $derived(items.filter((i) => i.entry.start_time))
  const flexible = $derived(open.filter((i) => !i.entry.start_time))
  const closed = $derived(items.filter((i) => i.task.status !== 'open'))
  const plannedIds = $derived(new Set(items.map((i) => i.task.id)))
  // "N times a week/month" routines whose window includes this day (doable any day of it).
  const windowed = $derived(
    [...store.tasks.values()]
      .filter((t) => t.status === 'open' && t.window_end && t.occurrence_date! <= date && date <= t.window_end && !plannedIds.has(t.id))
      .sort((a, b) => a.title.localeCompare(b.title) || (a.occurrence_key ?? '').localeCompare(b.occurrence_key ?? '')),
  )
  const due = $derived(
    [...store.tasks.values()]
      .filter((t) => t.status === 'open' && !t.window_end && t.due_date && t.due_date <= date && !plannedIds.has(t.id) && !store.isUpcoming(t, date))
      .sort((a, b) => (a.due_date! < b.due_date! ? -1 : 1)),
  )

  const planned = $derived(store.isPlanned(date))
  const doneCount = $derived(closed.filter((i) => i.task.status === 'done').length)
  const minutesLeft = $derived(open.reduce((s, i) => s + (i.entry.duration_min ?? i.task.estimate_min ?? 0), 0))

  // Current time, refreshed every 30 s.
  let now = $state(nowHHMM(store.me?.timezone ?? 'UTC'))
  $effect(() => {
    const t = setInterval(() => (now = nowHHMM(store.me?.timezone ?? 'UTC')), 30_000)
    return () => clearInterval(t)
  })
  // For today only the rest of the day counts as free.
  const free = $derived(
    store.me ? dayLoad(store.me, store.dayEntries(date), (id) => store.tasks.get(id), isToday ? now : undefined, store.dayBusy(date)).free
      : 0,
  )
  const dayEvents = $derived(store.dayEvents(date))
  $effect(() => {
    store.loadNamedays().catch(() => {})
  })
  const namedays = $derived(store.namedaysOn(date))
  const mine = $derived(store.occasionsOn(date))
  // Only clashes involving a scheduled task are alerts; a block overlapping an event just
  // has less room (its edge turns red on the timeline) (D-58).
  const conflicts = $derived(store.dayConflicts(date).filter((c) => c.aKind === 'task' || c.bKind === 'task'))

  function blocksMenu(value: string) {
    if (value === 'add') {
      const startH = isToday ? Math.min(22, Number(now.slice(0, 2)) + 1) : 9
      store.createBlock(date, {
        title: 'Focus',
        start_time: `${String(startH).padStart(2, '0')}:00`,
        end_time: `${String(startH + 1).padStart(2, '0')}:00`,
        energy: null,
      })
    } else if (value === 'none') store.applyTemplate(date, null)
    else if (value) store.applyTemplate(date, value)
  }
  function fixMove(entryId: string) {
    const t = store.moveToFreeTime(entryId)
    toast(t ? `Moved to ${t}` : 'No free time left today', t ? 'info' : 'error')
  }
  function fixShorten(entryId: string) {
    if (!store.shortenToFit(entryId)) toast("Can't shorten it to fit: it starts during the other item", 'error')
  }
  const upcoming = $derived(
    open.filter((i) => i.entry.start_time && i.entry.start_time >= now).sort((a, b) => (a.entry.start_time! < b.entry.start_time! ? -1 : 1)),
  )
  const next = $derived(isToday ? (upcoming[0] ?? flexible[0] ?? null) : null)

  // Wide screens show the timeline beside the plan; phones show it above.
  // Very wide screens get a third column (tracking and the activity log).
  const mqx = window.matchMedia('(min-width: 1500px)')
  let xwide = $state(mqx.matches)
  $effect(() => {
    const on = () => (xwide = mqx.matches)
    mqx.addEventListener('change', on)
    return () => mqx.removeEventListener('change', on)
  })
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
    announce(`“${store.tasks.get(item.taskId)?.title}” is now number ${index + 1} in the plan`)
  }

  // Routine occurrences exist up to tomorrow; ask the server to fill in later days.
  $effect(() => {
    if (date > addDays(store.today, 1)) api.get(`/days/${date}`).catch(() => {})
  })

  let showDone = $state(false)
  const go = (d: string) => router.go(d === store.today ? '/' : `/day/${d}`)
</script>

{#snippet timeline()}
  <h2 class="section-title">
    <Icon name="clock" size={14} /> Timeline
    <select
      class="blocks-menu"
      aria-label="Time blocks"
      value=""
      onchange={(e) => {
        blocksMenu((e.currentTarget as HTMLSelectElement).value)
        ;(e.currentTarget as HTMLSelectElement).value = ''
      }}>
      <option value="">Blocks…</option>
      {#each store.templateList() as t (t.id)}<option value={t.id}>Use “{t.name}”</option>{/each}
      <option value="add">+ Add a block</option>
      {#if store.blocksOn(date).length}<option value="none">Remove all blocks</option>{/if}
    </select>
  </h2>
  {#if dayEvents.allDay.length}
    <ul class="allday" aria-label="All-day events">
      {#each dayEvents.allDay as d (d.event.id)}
        <li style:--cal={d.color} title={d.event.location ?? ''}>{d.event.title}</li>
      {/each}
    </ul>
  {/if}
  <Timeline {date} items={timed} now={isToday ? now : null} />
{/snippet}

{#snippet side()}
  {#if date <= store.today}
    <QuickLog {date} />
    {#key date}<DayLog {date} open={date < store.today} />{/key}
  {/if}
{/snippet}

<div class="day" class:wide class:xwide={wide && xwide}>
  <div class="main-col">
    {#if isToday}<PlanBanner />{/if}
    {#if isToday && store.goalReviewDue()}
      <a class="card review-banner" href="/goals/review"><Icon name="flag" size={16} /> Time for your {store.me?.review_cadence === 'monthly' ? 'monthly' : 'weekly'} goals review <Icon name="right" size={16} /></a>
    {/if}
    <header class="head">
      <div class="title">
        <h1>{dayLabel(date, store.today)}</h1>
        <p class="muted">
          {longDate(date)}
          {#if namedays.length || mine.length}
            <span class="namedays" title="Namedays">
              {#each mine as o (o.person.id + o.kind)}<strong class="occasion"><Icon name="gift" size={12} /> {o.person.name}{o.kind === 'birthday' ? ' (birthday)' : ''}</strong>{/each}
              {#if namedays.length}<a href="/occasions">Nameday: {namedays.join(', ')}</a>{/if}
            </span>
          {/if}
          {#if planned}
            <a class="plan-chip done" href="/plan/{date}" title="Planned — open the planner to adjust"><Icon name="check" size={12} /> Planned</a>
          {:else if date >= store.today}
            <a class="plan-chip" href="/plan/{date}">Plan this day</a>
          {/if}
        </p>
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
        <span class="muted">{doneCount}/{items.length} done{minutesLeft ? ` · ${fmtMinutes(minutesLeft)} left` : ''}{date >= store.today ? ` · ${fmtMinutes(free)} free` : ''}</span>
      </div>
    {/if}

    {#if conflicts.length}
      <div class="conflicts card" role="alert">
        <strong><Icon name="clock" size={14} /> {conflicts.length === 1 ? 'An overlap' : `${conflicts.length} overlaps`}</strong>
        <ul>
          {#each conflicts as c (c.a + c.b)}
            {@const entry = c.aKind === 'task' ? c.a : c.bKind === 'task' ? c.b : null}
            <li>
              <span>“{store.itemTitle(c.a)}” and “{store.itemTitle(c.b)}” overlap by {fmtMinutes(c.minutes)}</span>
              {#if entry}
                <span class="fixes">
                  <button class="btn small" onclick={() => fixMove(entry)}>Move to free time</button>
                  <button class="btn small" onclick={() => fixShorten(entry)}>Shorten</button>
                  <button class="btn small" onclick={() => store.updateEntry(entry, { start_time: null })}>Unschedule</button>
                </span>
              {:else}<span class="muted small">Move or resize the block on the timeline.</span>{/if}
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    {#if next}
      <div class="next card">
        <button class="next-main" onclick={() => (ui.editing = next.task.id)}>
          <span class="label">{next.entry.start_time ? `Next · ${next.entry.start_time}` : 'Up next'}</span>
          <span class="next-title">{next.task.title}</span>
        </button>
        <button
          class="btn small"
          onclick={() => {
            store.focus('start', next.task.id)
            router.go('/focus')
          }}><Icon name="target" size={14} /> Focus</button>
      </div>
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

    {#if windowed.length}
      <h2 class="section-title"><Icon name="repeat" size={14} /> This week / month</h2>
      <div class="card list">
        {#each windowed as t (t.id)}
          <TaskRow task={t} showProject planButton planDate={date} />
        {/each}
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

    {#if !(wide && xwide)}{@render side()}{/if}

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

  {#if wide && xwide}<aside class="side-col">{@render side()}</aside>{/if}
  {#if wide}
    <aside class="time-col">{@render timeline()}</aside>
  {/if}
</div>

<style>
  .review-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    margin-bottom: 12px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-soft);
  }
  .blocks-menu {
    margin-left: auto;
    width: auto;
    font-size: 12px;
    padding: 2px 6px;
    text-transform: none;
    letter-spacing: 0;
  }
  .conflicts {
    border-left: 4px solid var(--danger);
    background: var(--danger-soft);
    padding: 10px 14px;
    margin-bottom: 14px;
    font-size: 14px;
  }
  .conflicts ul {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
  }
  .conflicts li {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 10px;
    align-items: center;
    padding: 4px 0;
  }
  .fixes {
    display: inline-flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .small {
    font-size: 12px;
  }
  .namedays {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px 8px;
    align-items: center;
    font-size: 13px;
  }
  .namedays a {
    color: var(--muted);
  }
  .occasion {
    display: inline-flex;
    gap: 3px;
    align-items: center;
    color: var(--accent);
  }
  .allday {
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0 0 8px;
    padding: 0;
  }
  .allday li {
    font-size: 13px;
    font-weight: 600;
    padding: 3px 10px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--cal) 16%, var(--surface));
    border-left: 3px solid var(--cal);
  }
  .day.wide {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(360px, 32%);
    gap: 28px;
    align-items: start;
  }
  .day.xwide {
    grid-template-columns: minmax(0, 1fr) minmax(320px, 24%) minmax(380px, 28%);
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
  .plan-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 8px;
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    background: var(--surface-2);
    color: var(--accent);
  }
  .plan-chip.done {
    background: var(--accent-soft);
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
    align-items: center;
    gap: 10px;
    padding: 14px 16px;
    margin-bottom: 14px;
    border-left: 4px solid var(--accent);
    box-shadow: var(--shadow);
  }
  .next-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    text-align: left;
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
  .link {
    color: var(--accent);
    font-weight: 600;
  }
  .list:global(.drop-hover) .empty {
    background: var(--accent-soft);
  }
</style>
