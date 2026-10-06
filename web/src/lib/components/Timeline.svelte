<script lang="ts">
  // Day timeline: drop tasks on it to give them a time, drag blocks to move them,
  // drag a block's bottom edge to change its duration (15-minute steps).
  import { tick, untrack } from 'svelte'
  import type { DayEntry } from '../api/types/DayEntry'
  import type { Task } from '../api/types/Task'
  import { fmtMinutes } from '../dates'
  import { announce } from '../announce.svelte'
  import { draggable, droppable, type DragItem } from '../dnd.svelte'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'
  import Check from './Check.svelte'

  type Item = { entry: DayEntry; task: Task }
  let { date, items, now = null }: { date: string; items: Item[]; now?: string | null } = $props()

  const PX = 0.9 // pixels per minute (54 px per hour)
  const SNAP = 15
  const toMin = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3, 5))
  const hhmm = (m: number) => `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`
  const durOf = (i: Item) => i.entry.duration_min ?? i.task.estimate_min ?? 30

  let scroller: HTMLDivElement
  let grid: HTMLDivElement
  let preview = $state<{ start: number; dur: number } | null>(null)
  let resizing = $state<{ id: string; dur: number } | null>(null)

  type Block = Item & { start: number; dur: number; lane: number; lanes: number }
  const blocks = $derived.by(() => {
    const bs: Block[] = items
      .map((i) => ({
        ...i,
        start: toMin(i.entry.start_time!),
        dur: resizing?.id === i.entry.id ? resizing.dur : durOf(i),
        lane: 0,
        lanes: 1,
      }))
      .sort((a, b) => a.start - b.start || b.dur - a.dur)
    // Side-by-side lanes for overlapping blocks, per cluster of overlaps.
    let cluster: Block[] = []
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
    return bs
  })

  const nowMin = $derived(now ? toMin(now) : null)

  // Scroll to "now" (today) or the first scheduled item when the day changes.
  $effect(() => {
    date
    untrack(() => {
      const first = blocks[0]?.start
      const target = nowMin !== null ? nowMin - 60 : (first ?? 8 * 60) - 30
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
      <div class="lanes">
        {#if preview}
          <div class="preview" style:top="{preview.start * PX}px" style:height="{Math.max(preview.dur, SNAP) * PX}px">
            {hhmm(preview.start)}–{hhmm(Math.min(preview.start + preview.dur, 1440))}
          </div>
        {/if}
        {#each blocks as b (b.entry.id)}
          <div
            class="block"
            class:done={b.task.status !== 'open'}
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
              <span class="title">{b.task.title}</span>
              <span class="time">{b.entry.start_time}–{hhmm(Math.min(b.start + b.dur, 1440))} · {fmtMinutes(b.dur)}</span>
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
      {#if nowMin !== null}
        <div class="now" style:top="{nowMin * PX}px"><span>{now}</span></div>
      {/if}
    </div>
  </div>
</div>

<style>
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
