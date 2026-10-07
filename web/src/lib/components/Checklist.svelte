<script lang="ts">
  // A checklist inside a task or routine (D-71): tick, edit, reorder, add, remove.
  import type { ChecklistItem } from '../api/types/ChecklistItem'
  import { ulid } from '../ulid'
  import Icon from './Icon.svelte'

  let {
    items,
    onchange,
    ontick,
    tickable = true,
    label = 'Checklist',
  }: {
    items: ChecklistItem[]
    onchange: (items: ChecklistItem[]) => void
    /** Ticking (tasks); defaults to saving the list with that item flipped. */
    ontick?: (itemId: string) => void
    /** Routines only define the steps; occurrences get ticked. */
    tickable?: boolean
    label?: string
  } = $props()

  let draft = $state('')
  const done = $derived(items.filter((i) => i.done).length)

  function add() {
    const text = draft.trim()
    if (!text || items.length >= 50) return
    onchange([...items, { id: ulid(), text, done: false }])
    draft = ''
  }
  function tick(id: string) {
    if (ontick) ontick(id)
    else onchange(items.map((i) => (i.id === id ? { ...i, done: !i.done } : i)))
  }
  function rename(id: string, text: string) {
    const t = text.trim()
    const cur = items.find((i) => i.id === id)
    if (!cur || t === cur.text) return
    onchange(t ? items.map((i) => (i.id === id ? { ...i, text: t } : i)) : items.filter((i) => i.id !== id))
  }
  function move(index: number, by: number) {
    const to = index + by
    if (to < 0 || to >= items.length) return
    const next = [...items]
    const [it] = next.splice(index, 1)
    next.splice(to, 0, it)
    onchange(next)
  }
</script>

<div class="checklist">
  {#if tickable && items.length}
    <div class="progress" aria-label="{done} of {items.length} done">
      <div class="bar"><div style:width="{(done / items.length) * 100}%"></div></div>
      <span class="muted">{done}/{items.length}</span>
    </div>
  {/if}
  <ul aria-label={label}>
    {#each items as it, i (it.id)}
      <li class:done={tickable && it.done}>
        {#if tickable}
          <input type="checkbox" checked={it.done} onchange={() => tick(it.id)} aria-label="{it.done ? 'Untick' : 'Tick'} {it.text}" />
        {:else}
          <span class="bullet" aria-hidden="true">•</span>
        {/if}
        <input
          class="text"
          type="text"
          value={it.text}
          maxlength="200"
          aria-label="Step {i + 1}"
          onblur={(e) => rename(it.id, (e.currentTarget as HTMLInputElement).value)}
          onkeydown={(e) => {
            if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur()
            else if (e.altKey && e.key === 'ArrowUp') (e.preventDefault(), move(i, -1))
            else if (e.altKey && e.key === 'ArrowDown') (e.preventDefault(), move(i, 1))
          }} />
        <span class="acts">
          <button class="icon-btn" disabled={i === 0} onclick={() => move(i, -1)} aria-label="Move {it.text} up"><Icon name="left" size={14} /></button>
          <button class="icon-btn" disabled={i === items.length - 1} onclick={() => move(i, 1)} aria-label="Move {it.text} down"><Icon name="right" size={14} /></button>
          <button class="icon-btn" onclick={() => onchange(items.filter((x) => x.id !== it.id))} aria-label="Remove {it.text}"><Icon name="x" size={14} /></button>
        </span>
      </li>
    {/each}
  </ul>
  {#if items.length < 50}
    <form
      class="add"
      onsubmit={(e) => {
        e.preventDefault()
        add()
      }}>
      <Icon name="plus" size={14} />
      <input type="text" bind:value={draft} maxlength="200" placeholder={items.length ? 'Add a step…' : 'Add a checklist step…'} aria-label="New step" />
    </form>
  {/if}
</div>

<style>
  .checklist {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }
  .bar {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--ok, var(--accent));
    transition: width 200ms ease;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 36px;
  }
  li input[type='checkbox'] {
    width: 18px;
    height: 18px;
    flex: none;
    accent-color: var(--accent);
  }
  .bullet {
    width: 18px;
    text-align: center;
    color: var(--faint);
  }
  .text {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    padding: 4px 2px;
  }
  .text:focus {
    background: var(--surface-2);
  }
  li.done .text {
    text-decoration: line-through;
    color: var(--muted);
  }
  .acts {
    display: flex;
    opacity: 0;
    transition: opacity 120ms;
  }
  .acts .icon-btn {
    width: 26px;
    height: 26px;
  }
  .acts .icon-btn:nth-child(1) :global(svg) {
    transform: rotate(90deg);
  }
  .acts .icon-btn:nth-child(2) :global(svg) {
    transform: rotate(90deg);
  }
  li:hover .acts,
  li:focus-within .acts {
    opacity: 1;
  }
  @media (hover: none) {
    .acts {
      opacity: 1;
    }
  }
  .add {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }
  .add input {
    flex: 1;
    border: 0;
    background: transparent;
    padding: 6px 2px;
  }
</style>
