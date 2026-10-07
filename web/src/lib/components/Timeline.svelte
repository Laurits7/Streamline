<script lang="ts">
  // Day timeline: drop tasks on it to give them a time, drag blocks to move them,
  // drag a block's bottom edge to change its duration (15-minute steps). Calendar events
  // are shown alongside, read-only.
  import { tick, untrack } from 'svelte'
  import type { DayEntry } from '../api/types/DayEntry'
  import type { Task } from '../api/types/Task'
  import { fmtMinutes } from '../dates'
  import { announce } from '../announce.svelte'
  import { timeRange, type DayEvent } from '../calendar'
  import { draggable, droppable, type DragItem } from '../dnd.svelte'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'
  import BlockSheet from './BlockSheet.svelte'
  import Check from './Check.svelte'
  import Icon from './Icon.svelte'

  type Item = { entry: DayEntry; task: Task }
  let {
    date,
    items,
    now = null,
    markers = [],
  }: {
    date: string
    items: Item[]
    now?: string | null
    /** Check-back times of tasks waiting for results (D-70). */
    markers?: { id: string; title: string; time: string }[]
  } = $props()
  const events = $derived(store.dayEvents(date).timed)
  const timeBlocks = $derived(store.blocksOn(date))
  // Tasks and events are marked only for clashes involving a task; blocks for any (D-58).
  const dayConflicts = $derived(store.dayConflicts(date))
  const clashes = $derived(
    new Set(dayConflicts.filter((c) => c.aKind === 'task' || c.bKind === 'task').flatMap((c) => [c.a, c.b])),
  )
  const blockClashes = $derived(new Set(dayConflicts.flatMap((c) => [c.a, c.b])))
  let editingBlock = $state<string | null>(null)
  let moving = $state<{ id: string; start: number; end: number } | null>(null)

  const PX = 0.9 // pixels per minute (54 px per hour)
  const SNAP = 15
  const toMin = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3, 5))
  const hhmm = (m: number) => `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`
  const durOf = (i: Item) => i.entry.duration_min ?? i.task.estimate_min ?? 30

  let scroller: HTMLDivElement
  let grid: HTMLDivElement
  let preview = $state<{ start: number; dur: number } | null>(null)
  let resizing = $state<{ id: string; dur: number } | null>(null)

  type Slot = { start: number; dur: number; lane: number; lanes: number }
  type Block = Item & Slot
  type EventBlock = DayEvent & Slot
  const layout = $derived.by(() => {
    const tasks: Block[] = items.map((i) => ({
      ...i,
      start: toMin(i.entry.start_time!),
      dur: resizing?.id === i.entry.id ? resizing.dur : durOf(i),
      lane: 0,
      lanes: 1,
    }))
    const evs: EventBlock[] = events.map((d) => ({ ...d, lane: 0, lanes: 1 }))
    const bs: Slot[] = [...evs, ...tasks].sort((a, b) => a.start - b.start || b.dur - a.dur)
    // Side-by-side lanes for overlapping blocks, per cluster of overlaps.
    let cluster: Slot[] = []
    let clusterEnd = -1
    const close = () => {
      const n = Math.max(1, ...cluster.map((b) => b.lane + 1))
      cluster.forEach((b) => (b.lanes = n))
    }
    for (const b of bs) {
      if (b.start >= clusterEnd && cluster.length) {
        close()
        cluster = []
      }
      const laneEnds: number[] = []
      for (const c of cluster) laneEnds[c.lane] = Math.max(laneEnds[c.lane] ?? 0, c.start + Math.max(c.dur, SNAP))
      let lane = 0
      while (laneEnds[lane] !== undefined && laneEnds[lane] > b.start) lane++
      b.lane = lane
      cluster.push(b)
      clusterEnd = Math.max(clusterEnd, b.start + Math.max(b.dur, SNAP))
    }
    if (cluster.length) close()
    return { tasks, evs }
  })
  const blocks = $derived(layout.tasks)
  const eventBlocks = $derived(layout.evs)

  const nowMin = $derived(now ? toMin(now) : null)

  // Scroll to "now" (today) or the first scheduled item when the day changes.
  $effect(() => {
    date
    untrack(() => {
      const first = Math.min(blocks[0]?.start ?? 1440, eventBlocks[0]?.start ?? 1440)
      const target = nowMin !== null ? nowMin - 60 : (first < 1440 ? first : 8 * 60) - 30
      tick().then(() => scroller && (scroller.scrollTop = Math.max(0, target * PX)))
    })
  })

  function minutesAt(y: number, item: DragItem) {
    const r = grid.getBoundingClientRect()
    const offset = item.kind === 'task' ? (item.grabOffsetMin ?? 0) : 0
    const m = Math.round(((y - r.top) / PX - offset) / SNAP) * SNAP
    return Math.max(0, Math.min(24 * 60 - SNAP, m))
  }

  function schedule(item: DragItem, start: number) {
    if (item.kind !== 'task') return
    const time = hhmm(start)
    announce(`Scheduled “${store.tasks.get(item.taskId)?.title}” at ${time}`)
    if (item.entryId && store.entries.get(item.entryId)?.date === date) store.updateEntry(item.entryId, { start_time: time })
    else store.plan(item.taskId, date, { startTime: time })
  }

  // Time blocks: drag the label to move (a click opens the editor), drag the handle to resize.
  function blockDrag(e: PointerEvent, id: string, start: number, end: number, mode: 'move' | 'resize') {
    e.preventDefault()
    e.stopPropagation()
    const el = e.currentTarget as HTMLElement
    el.setPointerCapture(e.pointerId)
    const y0 = e.clientY
    let moved = false
    const move = (ev: PointerEvent) => {
      const delta = Math.round((ev.clientY - y0) / PX / SNAP) * SNAP
      if (Math.abs(ev.clientY - y0) > 4) moved = true
      if (mode === 'move') {
        const s = Math.max(0, Math.min(1440 - (end - start), start + delta))
        moving = { id, start: s, end: s + (end - start) }
      } else moving = { id, start, end: Math.max(start + SNAP, Math.min(1440, end + delta)) }
    }
    const up = () => {
      el.removeEventListener('pointermove', move)
      el.removeEventListener('pointerup', up)
      el.removeEventListener('pointercancel', up)
      if (!moved && mode === 'move') editingBlock = id
      else if (moving && (moving.start !== start || moving.end !== end)) {
        store.updateBlock(id, { start_time: hhmm(moving.start), end_time: hhmm(Math.min(moving.end, 1439)) })
        announce(`Block moved to ${hhmm(moving.start)}–${hhmm(moving.end)}`)
      }
      moving = null
    }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', up)
    el.addEventListener('pointercancel', up)
  }

  function startResize(e: PointerEvent, b: Block) {
    e.preventDefault()
    e.stopPropagation()
    const el = e.currentTarget as HTMLElement
    el.setPointerCapture(e.pointerId)
    const y0 = e.clientY
    const base = b.dur
    resizing = { id: b.entry.id, dur: base }
    const move = (ev: PointerEvent) => {
      const dur = Math.round((base + (ev.clientY - y0) / PX) / SNAP) * SNAP
      resizing = { id: b.entry.id, dur: Math.max(SNAP, Math.min(24 * 60 - b.start, dur)) }
    }
    const up = () => {
      el.removeEventListener('pointermove', move)
      el.removeEventListener('pointerup', up)
      el.removeEventListener('pointercancel', up)
      if (resizing && resizing.dur !== base) store.updateEntry(b.entry.id, { duration_min: resizing.dur })
      resizing = null
    }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', up)
    el.addEventListener('pointercancel', up)
  }

  function resizeKey(e: KeyboardEvent, b: Block) {
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
    e.preventDefault()
    const dur = Math.max(SNAP, b.dur + (e.key === 'ArrowDown' ? SNAP : -SNAP))
    store.updateEntry(b.entry.id, { duration_min: dur })
  }
</script>

<div class="timeline card">
  <div class="scroller" bind:this={scroller} data-autoscroll>
    <div
      class="grid"
      bind:this={grid}
      style:height="{24 * 60 * PX}px"
      use:droppable={{
        accepts: (it) => it.kind === 'task',
        over: (it, at) => (preview = { start: minutesAt(at.y, it), dur: it.kind === 'task' ? it.durationMin : 30 }),
        leave: () => (preview = null),
        drop: (it, at) => {
          preview = null
          schedule(it, minutesAt(at.y, it))
        },
      }}>
      {#each Array.from({ length: 24 }, (_, h) => h) as h (h)}
        <div class="hour" style:top="{h * 60 * PX}px"><span>{String(h).padStart(2, '0')}:00</span></div>
      {/each}
      <div class="bands">
        {#each timeBlocks as tb (tb.id)}
          {@const s = moving?.id === tb.id ? moving.start : toMin(tb.start_time)}
          {@const en = moving?.id === tb.id ? moving.end : toMin(tb.end_time)}
          <div class="band" class:clash={blockClashes.has(tb.id)} style:top="{s * PX}px" style:height="{(en - s) * PX}px">
            <button
              class="band-label"
              data-nodrag
              title="{tb.title} · {hhmm(s)}–{hhmm(en)}. Drag to move, click to edit."
              onpointerdown={(e) => blockDrag(e, tb.id, toMin(tb.start_time), toMin(tb.end_time), 'move')}
              onkeydown={(e) => e.key === 'Enter' && (editingBlock = tb.id)}>
              {tb.title}{tb.energy ? ` · ${tb.energy}` : ''}
            </button>
            <div
              class="band-resize"
              data-nodrag
              role="slider"
              tabindex="0"
              aria-label="End of {tb.title}"
              aria-valuenow={en}
              aria-valuetext={hhmm(en)}
              onpointerdown={(e) => blockDrag(e, tb.id, toMin(tb.start_time), toMin(tb.end_time), 'resize')}
              onkeydown={(e) => {
                if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
                e.preventDefault()
                const end = Math.max(s + SNAP, Math.min(1439, en + (e.key === 'ArrowDown' ? SNAP : -SNAP)))
                store.updateBlock(tb.id, { end_time: hhmm(end) })
              }}>
            </div>
          </div>
        {/each}
      </div>
      <div class="lanes">
        {#if preview}
          <div class="preview" style:top="{preview.start * PX}px" style:height="{Math.max(preview.dur, SNAP) * PX}px">
            {hhmm(preview.start)}–{hhmm(Math.min(preview.start + preview.dur, 1440))}
          </div>
        {/if}
        {#each eventBlocks as ev (ev.event.id)}
          {@const project = store.eventProject(ev.event)}
          {@const todos = store.eventTasks(ev.event.id)}
          {@const openTodos = todos.filter((t) => t.status === 'open').length}
          <button
            type="button"
            class="event"
            class:short={ev.dur < 40}
            class:free={!ev.event.busy}
            class:clash={clashes.has(ev.event.id)}
            style:--cal={ev.color}
            style:top="{ev.start * PX}px"
            style:height="{Math.max(ev.dur, SNAP) * PX - 2}px"
            style:left="{(ev.lane / ev.lanes) * 100}%"
            style:width="calc({100 / ev.lanes}% - 3px)"
            title="{ev.event.title} · {timeRange(ev)}{ev.event.location ? ` · ${ev.event.location}` : ''}{project ? ` · ${project.name}` : ''}"
            onclick={() => (ui.event = ev.event.id)}>
            <span class="title">
              {ev.event.title}
              {#if todos.length}<span class="todos" class:all={!openTodos} aria-label="{openTodos} of {todos.length} todos open"><Icon name="check" size={11} />{todos.length - openTodos}/{todos.length}</span>{/if}
            </span>
            <span class="time">{timeRange(ev)}{ev.event.location ? ` · ${ev.event.location}` : ''}{#if project}<span class="proj">{' · '}<i style:background={project.color ?? 'var(--faint)'}></i>{project.name}</span>{/if}</span>
          </button>
        {/each}
        {#each blocks as b (b.entry.id)}
          <div
            class="block"
            class:done={b.task.status !== 'open'}
            class:clash={clashes.has(b.entry.id)}
            class:short={b.dur < 40}
            style:top="{b.start * PX}px"
            style:height="{Math.max(b.dur, SNAP) * PX - 2}px"
            style:left="{(b.lane / b.lanes) * 100}%"
            style:width="calc({100 / b.lanes}% - 3px)"
            data-id={b.entry.id}
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
            <span data-nodrag class="chk">
              <Check done={b.task.status === 'done'} onclick={() => store.toggleDone(b.task.id)} label="Complete {b.task.title}" />
            </span>
            <button
              class="body"
              title="{b.task.title} · {b.entry.start_time}–{hhmm(Math.min(b.start + b.dur, 1440))}"
              onclick={() => (ui.editing = b.task.id)}>
              <span class="title">{#if clashes.has(b.entry.id)}<span class="warn" aria-label="Overlaps">⚠</span> {/if}{b.task.title}</span>
              <span class="time">{b.entry.start_time}–{hhmm(Math.min(b.start + b.dur, 1440))} · {fmtMinutes(b.dur)}{b.task.checklist.length ? ` · ${b.task.checklist.filter((i) => i.done).length}/${b.task.checklist.length}` : ''}</span>
            </button>
            {#if b.task.status === 'open'}
              <div
                class="resize"
                data-nodrag
                role="slider"
                tabindex="0"
                aria-label="Duration of {b.task.title}"
                aria-valuenow={b.dur}
                aria-valuemin={SNAP}
                aria-valuemax={1440}
                aria-valuetext={fmtMinutes(b.dur)}
                onpointerdown={(e) => startResize(e, b)}
                onkeydown={(e) => resizeKey(e, b)}>
              </div>
            {/if}
          </div>
        {/each}
      </div>
      {#each markers as m (m.id)}
        <button class="checkback" style:top="{toMin(m.time) * PX}px" onclick={() => (ui.editing = m.id)} title="Check back on “{m.title}” at {m.time}">
          <span><Icon name="hourglass" size={10} /> {m.time} {m.title}</span>
        </button>
      {/each}
      {#if nowMin !== null}
        <div class="now" style:top="{nowMin * PX}px"><span>{now}</span></div>
      {/if}
    </div>
  </div>
</div>

{#if editingBlock}<BlockSheet id={editingBlock} onclose={() => (editingBlock = null)} />{/if}

<style>
  .bands {
    position: absolute;
    inset: 0 8px 0 48px;
    pointer-events: none;
  }
  .band {
    position: absolute;
    left: 0;
    right: 0;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    border-left: 3px dashed color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: 4px;
  }
  .band.clash {
    border-left-color: var(--danger);
  }
  .band-label {
    pointer-events: auto;
    position: absolute;
    right: 4px;
    top: 2px;
    max-width: 45%;
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    background: var(--surface);
    border-radius: 999px;
    padding: 1px 8px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: grab;
    touch-action: none;
    z-index: 2;
  }
  .band-resize {
    pointer-events: auto;
    position: absolute;
    right: 4px;
    bottom: 0;
    width: 36px;
    height: 8px;
    cursor: ns-resize;
    touch-action: none;
    z-index: 2;
  }
  .band-resize::after {
    content: '';
    position: absolute;
    left: 6px;
    right: 6px;
    bottom: 2px;
    height: 3px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .block.clash,
  .event.clash {
    outline: 2px solid var(--danger);
    outline-offset: -2px;
  }
  .warn {
    color: var(--danger);
  }
  .timeline {
    overflow: hidden;
  }
  .scroller {
    max-height: min(34vh, 320px);
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  :global(.time-col) .scroller {
    max-height: calc(100dvh - 140px);
  }
  .grid {
    position: relative;
  }
  .hour {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 1px solid var(--border);
    height: 0;
  }
  .hour span {
    position: absolute;
    left: 8px;
    top: 2px;
    font-size: 11px;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }
  .lanes {
    position: absolute;
    inset: 0 8px 0 52px;
  }
  .block {
    position: absolute;
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 4px 6px;
    border-radius: 8px;
    background: var(--accent-soft);
    border-left: 3px solid var(--accent);
    overflow: hidden;
    cursor: grab;
    font-size: 13px;
    touch-action: auto;
  }
  /* Calendar events: fixed, read-only, in the calendar's colour. */
  .event {
    position: absolute;
    display: flex;
    flex-direction: column;
    padding: 4px 8px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--cal) 16%, var(--surface));
    border-left: 3px solid var(--cal);
    overflow: hidden;
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }
  .event:hover {
    background: color-mix(in srgb, var(--cal) 24%, var(--surface));
  }
  .event .todos {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    margin-left: 4px;
    padding: 0 5px;
    border-radius: 8px;
    font-size: 11px;
    font-weight: 600;
    background: var(--surface);
    color: var(--muted);
  }
  .event .todos.all {
    color: var(--ok, #15803d);
  }
  .event .proj i {
    display: inline-block;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    margin-right: 3px;
  }
  .event.free {
    background: repeating-linear-gradient(
      135deg,
      color-mix(in srgb, var(--cal) 10%, var(--surface)) 0 6px,
      var(--surface) 6px 12px
    );
  }
  .block.done {
    opacity: 0.55;
    cursor: default;
  }
  .block.done .title {
    text-decoration: line-through;
  }
  .chk {
    display: contents;
  }
  .chk :global(.check) {
    width: 18px;
    height: 18px;
    margin-top: 1px;
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    text-align: left;
    cursor: inherit;
  }
  .short .time {
    display: none;
  }
  .title {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .time {
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .resize {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 8px;
    cursor: ns-resize;
    touch-action: none;
  }
  .resize::after {
    content: '';
    position: absolute;
    left: 50%;
    bottom: 2px;
    width: 24px;
    height: 3px;
    margin-left: -12px;
    border-radius: 2px;
    background: var(--accent);
    opacity: 0;
    transition: opacity 120ms;
  }
  .block:hover .resize::after,
  .resize:focus-visible::after {
    opacity: 0.6;
  }
  .preview {
    position: absolute;
    left: 0;
    right: 0;
    border-radius: 8px;
    border: 2px dashed var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
    font-size: 12px;
    font-weight: 700;
    padding: 2px 8px;
    pointer-events: none;
    z-index: 2;
  }
  .now {
    position: absolute;
    left: 48px;
    right: 0;
    border-top: 2px solid var(--danger);
    z-index: 3;
    pointer-events: none;
  }
  .checkback {
    position: absolute;
    left: 48px;
    right: 0;
    height: 0;
    padding: 0;
    border-top: 2px dashed var(--warn);
    z-index: 2;
  }
  .checkback span {
    position: absolute;
    left: 4px;
    top: -9px;
    max-width: calc(100% - 60px);
    display: inline-flex;
    align-items: center;
    gap: 3px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 10px;
    font-weight: 700;
    background: var(--warn-soft);
    color: var(--warn);
    border-radius: 999px;
    padding: 1px 6px;
  }
  /* Label at the right end, clear of the hour numbers on the left. */
  .now span {
    position: absolute;
    right: 4px;
    top: -9px;
    font-size: 10px;
    font-weight: 700;
    background: var(--danger);
    color: #fff;
    border-radius: 999px;
    padding: 1px 5px;
  }
</style>
