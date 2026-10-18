<script lang="ts">
  // One block of a day template (the week editor, D-113): its theme, times and kind of
  // work, and the routines and one-off tasks it plans. The menu alternative to drawing,
  // moving and resizing blocks and to dragging routines and tasks into them.
  import type { TemplateBlock } from '../api/types/TemplateBlock'
  import type { TemplateItem } from '../api/types/TemplateItem'
  import { describeSeries } from '../rrule'
  import { store } from '../store.svelte'
  import { ulid } from '../ulid'
  import { WEEKDAYS, currentRoutine } from '../weekTemplates'
  import Icon from './Icon.svelte'
  import Sheet from './Sheet.svelte'

  let {
    tplId,
    index,
    weekday = null,
    oncopy,
    onclose,
  }: {
    tplId: string
    index: number
    /** The weekday (0 = Monday) it was opened from, to offer a copy just for that day. */
    weekday?: number | null
    oncopy?: () => void
    onclose: () => void
  } = $props()

  const tpl = $derived(store.dayTemplates.get(tplId))
  const block = $derived(tpl?.blocks[index])
  // svelte-ignore state_referenced_locally
  let form = $state({ title: block?.title ?? '', start: block?.start ?? '09:00', end: block?.end ?? '10:00', energy: block?.energy ?? '' })
  const valid = $derived(form.title.trim() && form.start < form.end)
  const uses = $derived(store.templateUses(tplId))
  const shared = $derived(uses.length > 1)
  let task = $state({ title: '', estimate: '' })

  /** Routines worth adding: current versions that haven't ended; fixed-time ones plan themselves. */
  const routines = $derived(
    [...store.series.values()]
      .filter((s) => !s.deleted_at && s.mode !== 'anchored' && (!s.until || s.until >= store.today) && currentRoutine(s.id, store.series)?.id === s.id)
      .sort((a, b) => a.title.localeCompare(b.title)),
  )

  function patch(p: Partial<TemplateBlock>) {
    if (!tpl) return
    store.updateTemplate(tplId, { blocks: tpl.blocks.map((b, i) => (i === index ? { ...b, ...p } : b)) })
  }
  const setItems = (items: TemplateItem[]) => patch({ items })
  function addRoutine(e: Event) {
    const el = e.currentTarget as HTMLSelectElement
    const id = el.value
    el.value = ''
    if (!id || !block || block.items.some((i) => i.series_id === id)) return
    setItems([...block.items, { id: ulid(), kind: 'routine', series_id: id, title: null, estimate_min: null, notes: null }])
  }
  function addTask(e: Event) {
    e.preventDefault()
    const title = task.title.trim()
    if (!title || !block) return
    const est = Number(task.estimate)
    setItems([...block.items, { id: ulid(), kind: 'task', series_id: null, title, estimate_min: est > 0 ? Math.round(est) : null, notes: null }])
    task = { title: '', estimate: '' }
  }
  function save(e: Event) {
    e.preventDefault()
    if (!valid) return
    patch({ title: form.title.trim(), start: form.start, end: form.end, energy: form.energy || null })
    onclose()
  }
  function remove() {
    if (!tpl) return
    store.updateTemplate(tplId, { blocks: tpl.blocks.filter((_, i) => i !== index) })
    onclose()
  }
  const itemTitle = (it: TemplateItem) => (it.kind === 'task' ? (it.title ?? '') : (currentRoutine(it.series_id, store.series)?.title ?? 'Deleted routine'))
  const usedOn = $derived(uses.map(([w, i]) => `${WEEKDAYS[i]}${store.weekTemplates.size > 1 ? ` (${w.name})` : ''}`).join(', '))
</script>

<Sheet title="Time block" {onclose}>
  {#if tpl && block}
    {#if shared}
      <div class="shared">
        <p>“{tpl.name}” is used on {usedOn}: changes here apply to all of them.</p>
        {#if weekday !== null && oncopy}
          <button class="btn small" type="button" onclick={oncopy}>Make a copy for {WEEKDAYS[weekday]}</button>
        {/if}
      </div>
    {/if}
    <form onsubmit={save}>
      <label><span>Theme</span><input type="text" bind:value={form.title} required maxlength="100" /></label>
      <div class="two">
        <label><span>From</span><input type="time" bind:value={form.start} step="900" required /></label>
        <label><span>To</span><input type="time" bind:value={form.end} step="900" required /></label>
      </div>
      <label>
        <span>Kind of work (used by “Suggest times”)</span>
        <select bind:value={form.energy}>
          <option value="">Any</option>
          <option value="hard">Hard tasks</option>
          <option value="medium">Medium tasks</option>
          <option value="easy">Easy tasks</option>
        </select>
      </label>
      {#if form.start >= form.end}<p class="err">The block must end after it starts.</p>{/if}
      <div class="row">
        <button class="btn primary" type="submit" disabled={!valid}>Save</button>
        <button class="btn danger" type="button" onclick={remove}>Remove block</button>
      </div>
    </form>

    <h3>Planned here</h3>
    <p class="muted hint">
      Each day this template sets up, routines due that day (or with a slot left this week or month) and these tasks go into
      the block, one after another. Changes apply to days set up from now on.
    </p>
    <ul class="items">
      {#each block.items as it (it.id)}
        <li>
          <Icon name={it.kind === 'routine' ? 'repeat' : 'check'} size={14} />
          <span class="t">{itemTitle(it)}</span>
          {#if it.kind === 'routine'}
            {@const s = currentRoutine(it.series_id, store.series)}
            {#if s}<span class="muted sub">{describeSeries(s)}</span>{/if}
          {:else if it.estimate_min}
            <span class="muted sub">{it.estimate_min} min</span>
          {/if}
          <button class="icon-btn" type="button" aria-label="Remove {itemTitle(it)}" onclick={() => setItems(block.items.filter((x) => x.id !== it.id))}
            ><Icon name="x" size={14} /></button>
        </li>
      {:else}
        <li class="muted">Nothing yet.</li>
      {/each}
    </ul>
    {#if block.items.length < 20}
      <div class="add">
        <select value="" aria-label="Add a routine" onchange={addRoutine}>
          <option value="">Add routine…</option>
          {#each routines as s (s.id)}
            <option value={s.id} disabled={block.items.some((i) => i.series_id === s.id)}>{s.title} · {describeSeries(s)}</option>
          {/each}
        </select>
      </div>
      <form class="add task" onsubmit={addTask}>
        <input type="text" bind:value={task.title} placeholder="Add a task made each day…" maxlength="200" aria-label="Task title" />
        <input type="number" bind:value={task.estimate} min="1" max="1440" placeholder="min" aria-label="Estimate in minutes" />
        <button class="btn small" type="submit" disabled={!task.title.trim()}><Icon name="plus" size={14} /> Add</button>
      </form>
    {/if}
  {:else}
    <p class="muted">This block is gone.</p>
  {/if}
</Sheet>

<style>
  label {
    display: block;
    margin-bottom: 12px;
  }
  label span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .err {
    color: var(--danger);
    font-size: 13px;
  }
  .shared {
    padding: 8px 10px;
    margin-bottom: 12px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    font-size: 13px;
  }
  .shared p {
    margin: 0 0 6px;
  }
  h3 {
    font-size: 15px;
    margin: 18px 0 4px;
  }
  .hint {
    font-size: 12px;
    margin: 0 0 8px;
  }
  .items {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
  }
  .items li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
    font-size: 14px;
  }
  .items .t {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .items .sub {
    font-size: 12px;
  }
  .add {
    display: flex;
    gap: 6px;
    margin-bottom: 8px;
  }
  .add select,
  .add input[type='text'] {
    flex: 1;
    min-width: 0;
  }
  .add input[type='number'] {
    width: 70px;
  }
</style>
