<script lang="ts">
  // The week editor (D-113): pick a week template (the usual week, one every other week, or
  // one for dates), choose a day template per weekday, and edit the blocks on an hour grid:
  // draw a block by dragging over empty time, move it or drag its edges (15-minute steps),
  // and drag routines and new one-off tasks into it. Every drag has a menu alternative: the
  // block sheet (times, “Add routine…”, “Add a task”) and each column's “Add block”.
  import { tick, untrack } from 'svelte'
  import type { DayTemplate } from '../lib/api/types/DayTemplate'
  import type { TemplateBlock } from '../lib/api/types/TemplateBlock'
  import type { WeekTemplate } from '../lib/api/types/WeekTemplate'
  import { announce } from '../lib/announce.svelte'
  import Icon from '../lib/components/Icon.svelte'
  import TemplateBlockSheet from '../lib/components/TemplateBlockSheet.svelte'
  import { addDays, weekOf } from '../lib/dates'
  import { draggable, droppable, type DragItem } from '../lib/dnd.svelte'
  import { layoutLanes } from '../lib/lanes'
  import { describeSeries } from '../lib/rrule'
  import { store } from '../lib/store.svelte'
  import { ulid } from '../lib/ulid'
  import { MODE_LABEL, WEEKDAYS, currentRoutine, hhmm, toMin } from '../lib/weekTemplates'

  const PX = 0.8 // pixels per minute (48 px per hour), as in the week calendar
  const SNAP = 15
  const MAX = 24 * 60 - SNAP // blocks end by 23:45 (they don't cross midnight)

  const weeks = $derived(store.weekTemplateList())
  let chosen = $state<string | null>(null)
  const week = $derived(weeks.find((w) => w.id === chosen) ?? weeks.find((w) => w.mode === 'default') ?? weeks[0] ?? null)
  const hasDefault = $derived(weeks.some((w) => w.mode === 'default' && w.id !== week?.id))
  const hasAlternate = $derived(weeks.some((w) => w.mode === 'alternate' && w.id !== week?.id))
  const templates = $derived(store.templateList())

  type Slot = { j: number; b: TemplateBlock; start: number; dur: number; lane: number; lanes: number }
  const cols = $derived(
    WEEKDAYS.map((name, i) => {
      const id = week?.days[i] ?? null
      const tpl = id ? (store.dayTemplates.get(id) ?? null) : null
      const uses = tpl ? store.templateUses(tpl.id) : []
      return { i, name, tpl, uses }
    }),
  )

  // ---- drawing, moving and resizing blocks ---------------------------------------------

  type Gesture = { col: number; kind: 'draw' | 'move' | 'start' | 'end'; j: number | null; from: number; to: number; grab: number; moved: boolean }
  let g = $state<Gesture | null>(null)
  const colEls: HTMLDivElement[] = []
  const snap = (m: number) => Math.max(0, Math.min(24 * 60, Math.round(m / SNAP) * SNAP))
  const minuteAt = (col: number, y: number) => (y - colEls[col].getBoundingClientRect().top) / PX

  function slots(c: (typeof cols)[number]): Slot[] {
    const list = (c.tpl?.blocks ?? []).map((b, j) => {
      const live = g && g.col === c.i && g.j === j && g.kind !== 'draw' ? g : null
      const start = live ? live.from : toMin(b.start)
      const end = live ? live.to : toMin(b.end)
      return { j, b, start, dur: end - start, lane: 0, lanes: 1 }
    })
    return layoutLanes(list)
  }

  /** Mouse: drag straight away. Touch: long-press first, so the grid still scrolls. */
  function press(e: PointerEvent, col: number, kind: Gesture['kind'], j: number | null) {
    if (e.button !== 0 || !e.isPrimary || g) return
    const t = e.target as HTMLElement
    if (t.closest('[data-nodrag], button, input, select, a')) return
    if (kind === 'draw' && t.closest('.tblock')) return
    if (kind !== 'draw' && !week) return
    const b = j !== null ? cols[col].tpl?.blocks[j] : null
    const at = minuteAt(col, e.clientY)
    const start = b ? toMin(b.start) : snap(at)
    const end = b ? toMin(b.end) : snap(at)
    const sx = e.clientX
    const sy = e.clientY
    const immediate = e.pointerType === 'mouse' || kind === 'start' || kind === 'end'
    let timer: ReturnType<typeof setTimeout> | undefined
    let begun = false
    const begin = () => {
      begun = true
      navigator.vibrate?.(12)
      g = { col, kind, j, from: start, to: end, grab: at - start, moved: false }
    }
    const move = (ev: PointerEvent) => {
      if (!begun) {
        const dist = Math.hypot(ev.clientX - sx, ev.clientY - sy)
        if (immediate && dist > 4) begin()
        else if (!immediate && dist > 8) return cancel() // it's a scroll
        if (!begun) return
      }
      if (!g) return
      const m = minuteAt(col, ev.clientY)
      if (g.kind === 'draw') {
        const a = snap(at)
        const bnd = snap(m)
        g = { ...g, from: Math.min(a, bnd), to: Math.min(Math.max(a, bnd), MAX), moved: true }
      } else if (g.kind === 'move') {
        const dur = end - start
        const s = Math.max(0, Math.min(MAX - dur, snap(m - g.grab)))
        g = { ...g, from: s, to: s + dur, moved: g.moved || s !== start }
      } else if (g.kind === 'start') {
        g = { ...g, from: Math.max(0, Math.min(end - SNAP, snap(m))), moved: true }
      } else {
        g = { ...g, to: Math.min(MAX, Math.max(start + SNAP, snap(m))), moved: true }
      }
    }
    const up = () => {
      const done = g
      cancel()
      if (!begun) {
        // A tap on a block opens it.
        if (j !== null && cols[col].tpl) open(cols[col].tpl!.id, j, col)
        return
      }
      if (done) finish(done)
    }
    const cancel = () => {
      clearTimeout(timer)
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      window.removeEventListener('pointercancel', abort)
      g = null
    }
    const abort = () => cancel()
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    window.addEventListener('pointercancel', abort)
    if (!immediate) timer = setTimeout(begin, 350)
  }

  // A long-press drag must not scroll the page (the listener has to exist before the touch).
  function noScroll(node: HTMLElement) {
    const onMove = (e: TouchEvent) => g && e.preventDefault()
    node.addEventListener('touchmove', onMove, { passive: false })
    return { destroy: () => node.removeEventListener('touchmove', onMove) }
  }

  async function finish(done: Gesture) {
    const c = cols[done.col]
    if (done.kind === 'draw') {
      if (done.to - done.from < SNAP) return
      await addBlock(done.col, done.from, done.to)
      return
    }
    if (!c.tpl || done.j === null || !done.moved) return
    const tpl = c.tpl
    store.updateTemplate(tpl.id, {
      blocks: tpl.blocks.map((b, k) => (k === done.j ? { ...b, start: hhmm(done.from), end: hhmm(done.to) } : b)),
    })
    announce(`${tpl.blocks[done.j].title} ${hhmm(done.from)}–${hhmm(done.to)}`)
  }

  // ---- changes -------------------------------------------------------------------------

  const emptyWeek = (): (string | null)[] => [null, null, null, null, null, null, null]

  function setDay(i: number, id: string | null) {
    if (!week) return
    store.updateWeekTemplate(week.id, { days: week.days.map((d, k) => (k === i ? id : d)) })
  }

  /** A new day template for one weekday (when the column has none, or to stop sharing one). */
  async function newTemplateFor(i: number, blocks: TemplateBlock[], name = WEEKDAYS[i]) {
    if (!week) return null
    const id = ulid()
    await store.createTemplate(name, [], blocks, id)
    if (!store.dayTemplates.has(id)) return null
    setDay(i, id)
    return id
  }

  /** The next free hour of a column for a new block, as a menu alternative to drawing. */
  function freeHour(c: (typeof cols)[number]) {
    const blocks = c.tpl?.blocks ?? []
    for (let h = 9 * 60; h + 60 <= MAX; h += 60) if (!blocks.some((b) => toMin(b.start) < h + 60 && toMin(b.end) > h)) return h
    return 9 * 60
  }

  async function addBlock(col: number, start: number, end: number) {
    const c = cols[col]
    const b: TemplateBlock = { title: 'Focus', start: hhmm(start), end: hhmm(end), energy: null, items: [] }
    if (c.tpl) {
      store.updateTemplate(c.tpl.id, { blocks: [...c.tpl.blocks, b] })
      open(c.tpl.id, c.tpl.blocks.length, col)
    } else {
      const id = await newTemplateFor(col, [b])
      if (id) open(id, 0, col)
    }
  }

  /** Stop sharing: this weekday gets its own copy of the day template. */
  async function copyFor(i: number) {
    const tpl = cols[i].tpl
    if (!tpl) return
    const blocks = tpl.blocks.map((b) => ({ ...b, items: b.items.map((it) => ({ ...it, id: ulid() })) }))
    const id = await newTemplateFor(i, blocks, `${tpl.name} (${WEEKDAYS[i]})`)
    if (id && sheet) sheet = { ...sheet, tplId: id }
    if (id) announce(`${WEEKDAYS[i]} has its own copy of “${tpl.name}”`)
  }

  function dropItem(tpl: DayTemplate, j: number, it: DragItem) {
    if (it.kind !== 'template-item') return
    const b = tpl.blocks[j]
    if (b.items.length >= 20) return
    if (it.item.kind === 'routine' && b.items.some((x) => x.series_id === it.item.series_id)) return
    store.updateTemplate(tpl.id, {
      blocks: tpl.blocks.map((x, k) => (k === j ? { ...x, items: [...x.items, { id: ulid(), ...it.item }] } : x)),
    })
    const title = it.item.kind === 'routine' ? currentRoutine(it.item.series_id, store.series)?.title : it.item.title
    announce(`“${title}” goes into ${b.title}`)
  }

  function removeItem(tpl: DayTemplate, j: number, id: string) {
    store.updateTemplate(tpl.id, { blocks: tpl.blocks.map((x, k) => (k === j ? { ...x, items: x.items.filter((i) => i.id !== id) } : x)) })
  }

  function newWeek() {
    const today = store.today
    const days = week ? [...week.days] : emptyWeek()
    const next = weekOf(addDays(today, 7), 1)
    const p = !weeks.some((w) => w.mode === 'default')
      ? store.createWeekTemplate({ name: 'Usual week', days, mode: 'default', start_date: null, end_date: null })
      : store.createWeekTemplate({ name: 'Holiday week', days, mode: 'range', start_date: next, end_date: addDays(next, 6) })
    p?.then(() => (chosen = store.weekTemplateList().at(-1)?.id ?? chosen))
  }

  function setMode(w: WeekTemplate, mode: WeekTemplate['mode']) {
    const start = w.start_date ?? weekOf(store.today, 1)
    if (mode === 'default') store.updateWeekTemplate(w.id, { mode, start_date: null, end_date: null })
    else if (mode === 'alternate') store.updateWeekTemplate(w.id, { mode, start_date: start, end_date: null })
    else store.updateWeekTemplate(w.id, { mode, start_date: start, end_date: w.end_date ?? addDays(start, 6) })
  }

  // ---- the side list: routines and new tasks to drag into blocks ----------------------

  let search = $state('')
  let newTask = $state({ title: '', estimate: '' })
  const routines = $derived(
    [...store.series.values()]
      .filter((s) => !s.deleted_at && s.mode !== 'anchored' && (!s.until || s.until >= store.today) && currentRoutine(s.id, store.series)?.id === s.id)
      .filter((s) => !search.trim() || s.title.toLowerCase().includes(search.trim().toLowerCase()))
      .sort((a, b) => a.title.localeCompare(b.title)),
  )
  const itemTitle = (kind: string, seriesId: string | null, title: string | null) =>
    kind === 'task' ? (title ?? '') : (currentRoutine(seriesId, store.series)?.title ?? 'Deleted routine')

  // ---- the sheet -----------------------------------------------------------------------

  let sheet = $state<{ tplId: string; index: number; weekday: number } | null>(null)
  const open = (tplId: string, index: number, weekday: number) => (sheet = { tplId, index, weekday })

  let scroller = $state<HTMLDivElement>()
  $effect(() => {
    void week?.id
    untrack(() => tick().then(() => scroller && (scroller.scrollTop = 7 * 60 * PX)))
  })
</script>

<header class="head">
  <div>
    <h1>Week templates</h1>
    <p class="muted sub">
      Which blocks each weekday gets, and the routines and tasks planned in them. Days are set up 8 days ahead; a day you
      change by hand keeps its changes. <a href="/week">Back to the week</a>
    </p>
  </div>
</header>

{#if !weeks.length}
  <div class="card empty">
    <p>Start with your usual week: choose a day template for each weekday, or draw blocks straight onto a day.</p>
    <button class="btn primary" onclick={newWeek}><Icon name="plus" size={16} /> Make my usual week</button>
  </div>
{:else if week}
  <div class="weeks" role="tablist" aria-label="Week templates">
    {#each weeks as w (w.id)}
      <button role="tab" class="chip" class:on={w.id === week.id} aria-selected={w.id === week.id} onclick={() => (chosen = w.id)}>
        {w.name} <span class="muted">· {MODE_LABEL[w.mode]}</span>
      </button>
    {/each}
    <button class="chip" onclick={newWeek}><Icon name="plus" size={14} /> New week</button>
  </div>

  <div class="settings card">
    <input
      class="name"
      type="text"
      value={week.name}
      maxlength="60"
      aria-label="Week template name"
      onchange={(e) => {
        const v = (e.currentTarget as HTMLInputElement).value.trim()
        if (v) store.updateWeekTemplate(week.id, { name: v })
      }} />
    <select value={week.mode} aria-label="When it applies" onchange={(e) => setMode(week, (e.currentTarget as HTMLSelectElement).value as WeekTemplate['mode'])}>
      <option value="default" disabled={hasDefault}>The usual week</option>
      <option value="alternate" disabled={hasAlternate}>Every other week</option>
      <option value="range">For dates</option>
    </select>
    {#if week.mode === 'alternate'}
      <label
        >Starting the week of <input
          type="date"
          value={week.start_date ?? ''}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLInputElement).value
            if (v) store.updateWeekTemplate(week.id, { start_date: v })
          }} /></label>
    {:else if week.mode === 'range'}
      <label
        >From <input
          type="date"
          value={week.start_date ?? ''}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLInputElement).value
            if (v) store.updateWeekTemplate(week.id, { start_date: v, end_date: week.end_date && week.end_date < v ? v : week.end_date })
          }} /></label>
      <label
        >to <input
          type="date"
          value={week.end_date ?? ''}
          min={week.start_date ?? undefined}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLInputElement).value
            if (v) store.updateWeekTemplate(week.id, { end_date: v })
          }} /></label>
    {/if}
    <button
      class="btn small danger"
      onclick={() => confirm(`Delete “${week.name}”? Days already set up keep their blocks until the next set-up.`) && (store.deleteWeekTemplate(week.id), (chosen = null))}
      ><Icon name="trash" size={14} /> Delete</button>
    <p class="muted help">
      {#if week.mode === 'default'}Used whenever no other week template applies.
      {:else if week.mode === 'alternate'}Used every other week, from the week of its start date, instead of the usual week.
      {:else}Used on these dates instead of the others (holidays, a trip…).{/if}
    </p>
  </div>

  <div class="editor">
    <div class="cal card">
      <div class="scroller" bind:this={scroller} data-autoscroll data-autoscroll-x>
        <div class="heads">
          <span class="gutter"></span>
          {#each cols as c (c.i)}
            <div class="colhead">
              <strong>{c.name}</strong>
              <select
                value={c.tpl?.id ?? ''}
                aria-label="Day template for {c.name}"
                onchange={(e) => {
                  const v = (e.currentTarget as HTMLSelectElement).value
                  if (v === '+') newTemplateFor(c.i, [])
                  else setDay(c.i, v || null)
                }}>
                <option value="">No blocks</option>
                {#each templates as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
                <option value="+">New day template…</option>
              </select>
              {#if c.tpl}
                <input
                  class="tname"
                  type="text"
                  value={c.tpl.name}
                  maxlength="60"
                  aria-label="Name of {c.name}'s day template"
                  onchange={(e) => {
                    const v = (e.currentTarget as HTMLInputElement).value.trim()
                    if (v && c.tpl) store.updateTemplate(c.tpl.id, { name: v })
                  }} />
                {#if c.uses.length > 1}
                  <span class="shared" title="Changing it changes every day that uses it">
                    Shared with {c.uses.length - 1} other day{c.uses.length > 2 ? 's' : ''}
                    <button class="link" onclick={() => copyFor(c.i)}>Copy for {c.name}</button>
                  </span>
                {/if}
              {/if}
              <button class="btn small" onclick={() => addBlock(c.i, freeHour(c), freeHour(c) + 60)}><Icon name="plus" size={13} /> Add block</button>
            </div>
          {/each}
        </div>
        <div class="grid" style:height="{24 * 60 * PX}px" use:noScroll>
          <div class="gutter hours">
            {#each Array.from({ length: 24 }, (_, h) => h) as h (h)}
              <span style:top="{h * 60 * PX}px">{String(h).padStart(2, '0')}:00</span>
            {/each}
          </div>
          {#each cols as c (c.i)}
            <div
              class="col"
              class:none={!c.tpl}
              bind:this={colEls[c.i]}
              role="presentation"
              onpointerdown={(e) => press(e, c.i, 'draw', null)}>
              {#each Array.from({ length: 24 }, (_, h) => h) as h (h)}<i class="line" style:top="{h * 60 * PX}px"></i>{/each}
              {#each slots(c) as s (s.j)}
                {@const tpl = c.tpl!}
                <div
                  class="tblock"
                  class:hard={s.b.energy === 'hard'}
                  class:easy={s.b.energy === 'easy'}
                  class:active={g?.col === c.i && g.j === s.j}
                  style:top="{s.start * PX}px"
                  style:height="{Math.max(s.dur, SNAP) * PX - 2}px"
                  style:left="{(s.lane / s.lanes) * 100}%"
                  style:width="calc({100 / s.lanes}% - 2px)"
                  role="button"
                  tabindex="0"
                  aria-label="{s.b.title}, {hhmm(s.start)} to {hhmm(s.start + s.dur)}. Open to edit"
                  onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), open(tpl.id, s.j, c.i))}
                  onpointerdown={(e) => (e.stopPropagation(), press(e, c.i, 'move', s.j))}
                  use:droppable={{
                    accepts: (it) => it.kind === 'template-item' && tpl.blocks[s.j].items.length < 20,
                    drop: (it) => dropItem(tpl, s.j, it),
                  }}>
                  <span class="edge top" data-edge aria-hidden="true" onpointerdown={(e) => (e.stopPropagation(), press(e, c.i, 'start', s.j))}></span>
                  <span class="bt">{s.b.title}</span>
                  <span class="when">{hhmm(s.start)}–{hhmm(s.start + s.dur)}</span>
                  {#each s.b.items as it (it.id)}
                    <span class="item" class:routine={it.kind === 'routine'}>
                      <Icon name={it.kind === 'routine' ? 'repeat' : 'check'} size={11} />
                      <span class="it">{itemTitle(it.kind, it.series_id, it.title)}</span>
                      <button class="x" data-nodrag aria-label="Remove {itemTitle(it.kind, it.series_id, it.title)}" onclick={() => removeItem(tpl, s.j, it.id)}
                        ><Icon name="x" size={11} /></button>
                    </span>
                  {/each}
                  <span class="edge bottom" data-edge aria-hidden="true" onpointerdown={(e) => (e.stopPropagation(), press(e, c.i, 'end', s.j))}></span>
                </div>
              {/each}
              {#if g?.kind === 'draw' && g.col === c.i && g.to > g.from}
                <div class="draft" style:top="{g.from * PX}px" style:height="{(g.to - g.from) * PX}px">{hhmm(g.from)}–{hhmm(g.to)}</div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    </div>

    <aside class="side card" aria-label="Routines and tasks">
      <h2>Plan in blocks</h2>
      <p class="muted hint">Drag into a block, or open a block to add from a list.</p>
      <div class="newtask">
        <input type="text" bind:value={newTask.title} placeholder="New task title" maxlength="200" aria-label="New task title" />
        <input type="number" bind:value={newTask.estimate} min="1" max="1440" placeholder="min" aria-label="Estimate in minutes" />
      </div>
      <div
        class="pick task"
        class:off={!newTask.title.trim()}
        use:draggable={{
          disabled: !newTask.title.trim(),
          item: () => ({
            kind: 'template-item',
            item: { kind: 'task', series_id: null, title: newTask.title.trim(), estimate_min: Number(newTask.estimate) > 0 ? Math.round(Number(newTask.estimate)) : null, notes: null },
          }),
        }}>
        <Icon name="grip" size={13} /> <Icon name="check" size={13} />
        <span class="pt">{newTask.title.trim() || 'Type a title, then drag'}</span>
      </div>
      <label class="search"><Icon name="search" size={14} /><input type="search" bind:value={search} placeholder="Routines" aria-label="Search routines" /></label>
      <ul>
        {#each routines as s (s.id)}
          <li
            class="pick"
            use:draggable={{ item: () => ({ kind: 'template-item', item: { kind: 'routine', series_id: s.id, title: null, estimate_min: null, notes: null } }) }}>
            <Icon name="grip" size={13} /> <Icon name="repeat" size={13} />
            <span class="pt">{s.title}</span>
            <span class="muted ps">{describeSeries(s)}</span>
          </li>
        {:else}
          <li class="muted">{search ? 'No routine matches.' : 'No routines yet.'}</li>
        {/each}
      </ul>
      <p class="muted hint">Fixed-time routines aren't listed: they're planned at their own time.</p>
    </aside>
  </div>
{/if}

{#if sheet}
  <TemplateBlockSheet tplId={sheet.tplId} index={sheet.index} weekday={sheet.weekday} oncopy={() => sheet && copyFor(sheet.weekday)} onclose={() => (sheet = null)} />
{/if}

<style>
  .head {
    margin-bottom: 12px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .sub {
    margin: 2px 0 0;
    font-size: 14px;
  }
  .empty {
    padding: 16px;
  }
  .weeks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 10px;
  }
  .settings {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    margin-bottom: 12px;
  }
  .settings .name {
    font-weight: 650;
    min-width: 0;
    flex: 1 1 160px;
  }
  .settings label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .settings .help {
    flex-basis: 100%;
    margin: 0;
    font-size: 13px;
  }
  .editor {
    display: grid;
    grid-template-columns: 1fr 220px;
    gap: 12px;
    align-items: start;
  }
  @media (max-width: 900px) {
    .editor {
      grid-template-columns: 1fr;
    }
  }
  .cal {
    overflow: hidden;
    padding: 0;
    min-width: 0;
  }
  .scroller {
    max-height: calc(100dvh - 260px);
    min-height: 380px;
    overflow: auto;
    overscroll-behavior: contain;
  }
  .heads {
    position: sticky;
    top: 0;
    z-index: 5;
    display: grid;
    grid-template-columns: 48px repeat(7, minmax(132px, 1fr));
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .colhead {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
    min-width: 0;
    border-left: 1px solid var(--border);
    font-size: 13px;
  }
  .colhead select,
  .colhead .tname {
    width: 100%;
    font-size: 12px;
    padding: 3px 4px;
  }
  .colhead .shared {
    font-size: 11px;
    color: var(--muted);
  }
  .link {
    color: var(--accent);
    font-size: 11px;
    text-decoration: underline;
  }
  .grid {
    position: relative;
    display: grid;
    grid-template-columns: 48px repeat(7, minmax(132px, 1fr));
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
    cursor: crosshair;
    user-select: none;
  }
  .col.none {
    background: var(--bg);
  }
  .line {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 1px solid var(--border);
    pointer-events: none;
  }
  .tblock {
    position: absolute;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 4px 5px;
    border-radius: 6px;
    overflow: hidden;
    font-size: 12px;
    line-height: 1.25;
    background: color-mix(in srgb, var(--accent) 12%, var(--surface));
    border-left: 3px solid var(--accent);
    cursor: grab;
  }
  .tblock.hard {
    border-left-color: var(--danger);
  }
  .tblock.easy {
    border-left-color: var(--ok);
  }
  .tblock.active {
    box-shadow: var(--shadow);
    z-index: 3;
  }
  .tblock .bt {
    font-weight: 650;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tblock .when {
    font-size: 10px;
    color: var(--muted);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    padding: 1px 3px;
    border-radius: 4px;
    background: var(--surface);
  }
  .item .it {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item .x {
    display: flex;
    padding: 1px;
    color: var(--muted);
  }
  .edge {
    position: absolute;
    left: 0;
    right: 0;
    height: 8px;
    cursor: ns-resize;
    touch-action: none;
  }
  .edge.top {
    top: 0;
  }
  .edge.bottom {
    bottom: 0;
  }
  .draft {
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
  .side {
    padding: 10px;
    position: sticky;
    top: 12px;
    max-height: calc(100dvh - 140px);
    overflow-y: auto;
  }
  .side h2 {
    font-size: 15px;
    margin: 0;
  }
  .hint {
    font-size: 12px;
    margin: 4px 0 8px;
  }
  .newtask {
    display: flex;
    gap: 4px;
    margin-bottom: 4px;
  }
  .newtask input[type='text'] {
    flex: 1;
    min-width: 0;
  }
  .newtask input[type='number'] {
    width: 60px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 10px 0 6px;
  }
  .search input {
    flex: 1;
    min-width: 0;
  }
  .side ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 4px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    margin-bottom: 4px;
    font-size: 13px;
    cursor: grab;
    background: var(--surface);
  }
  .pick.off {
    opacity: 0.55;
    cursor: default;
  }
  .pick .pt {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pick .ps {
    font-size: 11px;
    white-space: nowrap;
  }
</style>
