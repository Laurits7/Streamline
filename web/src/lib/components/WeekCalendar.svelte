<script lang="ts">
  // Week calendar (D-94): seven days on an hour grid with calendar events, timed tasks and
  // time blocks. Drag a task to another day or time (15-minute steps); click to open it.
  import { tick, untrack } from 'svelte'
  import type { DayEntry } from '../api/types/DayEntry'
  import type { Task } from '../api/types/Task'
  import { announce } from '../announce.svelte'
  import { timeRange, type DayEvent } from '../calendar'
  import { dayLabel, fmtMinutes, nowHHMM } from '../dates'
  import { draggable, droppable, type DragItem } from '../dnd.svelte'
  import { layoutLanes } from '../lanes'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'

  let { days }: { days: string[] } = $props()

  const PX = 0.8 // pixels per minute (48 px per hour)
  const SNAP = 15
  const toMin = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3, 5))
  const hhmm = (m: number) => `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`

  type TaskBlock = { kind: 'task'; entry: DayEntry; task: Task; start: number; dur: number; lane: number; lanes: number }
  type EventBlock = { kind: 'event'; ev: DayEvent; start: number; dur: number; lane: number; lanes: number }

  function column(date: string) {
    const tasks: TaskBlock[] = store
      .dayEntries(date)
      .filter((e) => e.start_time)
      .map((e) => ({ entry: e, task: store.tasks.get(e.task_id) }))
      .filter((r): r is { entry: DayEntry; task: Task } => !!r.task && r.task.status !== 'missed' && r.task.status !== 'skipped')
      .map((r) => ({
        kind: 'task' as const,
        ...r,
        start: toMin(r.entry.start_time!),
        dur: r.entry.duration_min ?? r.task.estimate_min ?? 30,
        lane: 0,
        lanes: 1,
      }))
    const events: EventBlock[] = store.dayEvents(date).timed.map((ev) => ({ kind: 'event' as const, ev, start: ev.start, dur: ev.dur, lane: 0, lanes: 1 }))
    const all = layoutLanes<TaskBlock | EventBlock>([...events, ...tasks])
    const untimed = store.dayEntries(date).filter((e) => !e.start_time && store.tasks.get(e.task_id)?.status === 'open').length
    return { all, allDay: store.dayEvents(date).allDay, bands: store.blocksOn(date), untimed }
  }
  const cols = $derived(days.map((date) => ({ date, ...column(date) })))

  let scroller: HTMLDivElement
  const grids: Record<string, HTMLDivElement> = {}
  let preview = $state<{ date: string; start: number; dur: number } | null>(null)
  const now = $derived(days.includes(store.today) && store.me ? toMin(nowHHMM(store.me.timezone)) : null)

  // Start the view a little before now (this week) or at 7:00.
  $effect(() => {
    void days[0]
    untrack(() => tick().then(() => scroller && (scroller.scrollTop = Math.max(0, ((now ?? 8 * 60) - 60) * PX))))
  })

  function minutesAt(date: string, y: number, item: DragItem) {
    const r = grids[date].getBoundingClientRect()
    const offset = item.kind === 'task' ? (item.grabOffsetMin ?? 0) : 0
    const m = Math.round(((y - r.top) / PX - offset) / SNAP) * SNAP
    return Math.max(0, Math.min(24 * 60 - SNAP, m))
  }
  function schedule(item: DragItem, date: string, start: number) {
    if (item.kind !== 'task') return
    const time = hhmm(start)
    if (item.entryId && store.entries.get(item.entryId)?.date === date) store.updateEntry(item.entryId, { start_time: time })
    else store.plan(item.taskId, date, { startTime: time })
    announce(`“${store.tasks.get(item.taskId)?.title}” at ${time}, ${dayLabel(date, store.today)}`)
  }
</script>

<div class="cal card">
  <div class="scroller" bind:this={scroller}>
    <div class="heads">
      <span class="gutter"></span>
      {#each cols as c (c.date)}
        <div class="head" class:today={c.date === store.today}>
          <a href={c.date === store.today ? '/' : `/day/${c.date}`}>
            <strong>{dayLabel(c.date, store.today)}</strong>
            <span class="muted">{Number(c.date.slice(8))}</span>
          </a>
          {#each c.allDay as d (d.event.id)}
            <button class="allday" style:--cal={d.color} onclick={() => (ui.event = d.event.id)}>{d.event.title}</button>
          {/each}
          {#if c.untimed}<a class="untimed" href="/week" onclick={(e) => (e.preventDefault(), store.setPref('weekView', 'planner'))}>+{c.untimed} without a time</a>{/if}
        </div>
      {/each}
    </div>
    <div class="grid" style:height="{24 * 60 * PX}px">
      <div class="gutter hours">
        {#each Array.from({ length: 24 }, (_, h) => h) as h (h)}
          <span style:top="{h * 60 * PX}px">{String(h).padStart(2, '0')}:00</span>
        {/each}
      </div>
      {#each cols as c (c.date)}
        <div
          class="col"
          class:today={c.date === store.today}
          class:past={c.date < store.today}
          bind:this={grids[c.date]}
          use:droppable={{
            accepts: (it) => it.kind === 'task' && c.date >= store.today,
            over: (it, at) => (preview = { date: c.date, start: minutesAt(c.date, at.y, it), dur: it.kind === 'task' ? it.durationMin : 30 }),
            leave: () => preview?.date === c.date && (preview = null),
            drop: (it, at) => {
              preview = null
              schedule(it, c.date, minutesAt(c.date, at.y, it))
            },
          }}>
          {#each Array.from({ length: 24 }, (_, h) => h) as h (h)}<i class="line" style:top="{h * 60 * PX}px"></i>{/each}
          {#each c.bands as b (b.id)}
            <div class="band" style:top="{toMin(b.start_time) * PX}px" style:height="{(toMin(b.end_time) - toMin(b.start_time)) * PX}px" title="{b.title} · {b.start_time}–{b.end_time}">
              <span>{b.title}</span>
            </div>
          {/each}
          {#if preview?.date === c.date}
            <div class="preview" style:top="{preview.start * PX}px" style:height="{Math.max(preview.dur, SNAP) * PX}px">{hhmm(preview.start)}</div>
          {/if}
          {#each c.all as b (b.kind === 'task' ? b.entry.id : b.ev.event.id)}
            {#if b.kind === 'event'}
              <button
                class="item event"
                class:free={!b.ev.event.busy}
                style:--cal={b.ev.color}
                style:top="{b.start * PX}px"
                style:height="{Math.max(b.dur, SNAP) * PX - 2}px"
                style:left="{(b.lane / b.lanes) * 100}%"
                style:width="calc({100 / b.lanes}% - 2px)"
                title="{b.ev.event.title} · {timeRange(b.ev)}"
                onclick={() => (ui.event = b.ev.event.id)}>
                <span class="t">{b.ev.event.title}</span>
              </button>
            {:else}
              <button
                class="item task main"
                class:done={b.task.status !== 'open'}
                data-task={b.task.id}
                style:top="{b.start * PX}px"
                style:height="{Math.max(b.dur, SNAP) * PX - 2}px"
                style:left="{(b.lane / b.lanes) * 100}%"
                style:width="calc({100 / b.lanes}% - 2px)"
                title="{b.task.title} · {b.entry.start_time}, {fmtMinutes(b.dur)}"
                onclick={() => (ui.editing = b.task.id)}
                use:draggable={{
                  disabled: b.task.status !== 'open',
                  item: (down, node) => ({
                    kind: 'task',
                    taskId: b.task.id,
                    entryId: b.entry.id,
                    durationMin: b.dur,
                    grabOffsetMin: (down.clientY - node.getBoundingClientRect().top) / PX,
                  }),
                }}>
                <span class="t">{b.task.title}</span>
                {#if b.dur >= 40}<span class="when">{b.entry.start_time}</span>{/if}
              </button>
            {/if}
          {/each}
          {#if now !== null && c.date === store.today}<div class="now" style:top="{now * PX}px"></div>{/if}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .cal {
    overflow: hidden;
    padding: 0;
  }
  /* Inside the scroller, pinned to its top: day names and the grid scroll sideways together. */
  .heads {
    position: sticky;
    top: 0;
    z-index: 5;
    display: grid;
    grid-template-columns: 48px repeat(7, minmax(110px, 1fr));
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .head {
    padding: 6px 6px 4px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    border-left: 1px solid var(--border);
  }
  .head a {
    display: flex;
    justify-content: space-between;
    gap: 4px;
    color: inherit;
    font-size: 13px;
  }
  .head.today strong {
    color: var(--accent);
  }
  .allday {
    font-size: 11px;
    font-weight: 600;
    text-align: left;
    padding: 1px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--cal) 16%, var(--surface));
    border-left: 3px solid var(--cal);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .untimed {
    font-size: 11px;
    color: var(--muted);
  }
  .scroller {
    max-height: calc(100dvh - 230px);
    min-height: 360px;
    overflow: auto;
    overscroll-behavior: contain;
  }
  .grid {
    position: relative;
    display: grid;
    grid-template-columns: 48px repeat(7, minmax(110px, 1fr));
  }
  .hours {
    position: relative;
  }
  .hours span {
    position: absolute;
    left: 6px;
    font-size: 11px;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
    transform: translateY(2px);
  }
  .col {
    position: relative;
    border-left: 1px solid var(--border);
  }
  .col.today {
    background: color-mix(in srgb, var(--accent) 4%, transparent);
  }
  .col.past {
    background: var(--bg);
  }
  .line {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 1px solid var(--border);
  }
  .band {
    position: absolute;
    left: 0;
    right: 0;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    border-left: 3px dashed color-mix(in srgb, var(--accent) 45%, transparent);
    pointer-events: none;
  }
  .band span {
    position: absolute;
    right: 3px;
    top: 1px;
    font-size: 10px;
    font-weight: 600;
    color: var(--accent);
  }
  .item {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 2px 5px;
    border-radius: 6px;
    font-size: 12px;
    overflow: hidden;
    text-align: left;
    line-height: 1.25;
  }
  .item .t {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .item .when {
    font-size: 10px;
    color: var(--muted);
  }
  .task {
    background: var(--accent-soft);
    border-left: 3px solid var(--accent);
    cursor: grab;
    touch-action: auto;
  }
  .task.done {
    opacity: 0.55;
    cursor: default;
  }
  .task.done .t {
    text-decoration: line-through;
  }
  .event {
    background: color-mix(in srgb, var(--cal) 16%, var(--surface));
    border-left: 3px solid var(--cal);
  }
  .event.free {
    background: repeating-linear-gradient(135deg, color-mix(in srgb, var(--cal) 10%, var(--surface)) 0 6px, var(--surface) 6px 12px);
  }
  .preview {
    position: absolute;
    left: 0;
    right: 0;
    border: 2px dashed var(--accent);
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
    font-size: 11px;
    font-weight: 700;
    padding: 1px 4px;
    pointer-events: none;
    z-index: 2;
  }
  .now {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 2px solid var(--danger);
    z-index: 3;
    pointer-events: none;
  }
</style>
