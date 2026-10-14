<script lang="ts">
  // A task's checklist next to its timeline item (D-81): tick steps without opening the task.
  // Opened by hovering the item, long-pressing it on touch, or its "2/4" chip (keyboard).
  import { tick } from 'svelte'
  import { placePopover } from '../checklist'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'

  let {
    taskId,
    anchor,
    focus = false,
    onclose,
    onpointerenter,
    onpointerleave,
  }: {
    taskId: string
    anchor: HTMLElement
    /** Move focus into the popover (opened from the keyboard). */
    focus?: boolean
    onclose: () => void
    onpointerenter?: (e: PointerEvent) => void
    onpointerleave?: (e: PointerEvent) => void
  } = $props()

  const task = $derived(store.tasks.get(taskId))
  const done = $derived(task ? task.checklist.filter((i) => i.done).length : 0)
  let el = $state<HTMLDivElement>()
  let pos = $state<{ top: number; left: number } | null>(null)

  function place() {
    if (!el || !anchor.isConnected) return onclose()
    const r = anchor.getBoundingClientRect()
    pos = placePopover(r, { width: el.offsetWidth, height: el.offsetHeight }, { width: innerWidth, height: innerHeight })
  }

  $effect(() => {
    task?.checklist.length
    place()
  })
  $effect(() => {
    if (focus) tick().then(() => el?.querySelector<HTMLElement>('input, button')?.focus())
  })
  $effect(() => {
    if (!task || !task.checklist.length) onclose()
  })

  // Close on a tap/click elsewhere (the anchor itself toggles), Escape, or scrolling.
  $effect(() => {
    const down = (e: PointerEvent) => {
      const t = e.target as Node
      if (!el?.contains(t) && !anchor.contains(t)) onclose()
    }
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.stopPropagation()
      anchor.querySelector<HTMLElement>('.steps')?.focus()
      onclose()
    }
    const scroll = (e: Event) => !el?.contains(e.target as Node) && onclose()
    window.addEventListener('pointerdown', down, true)
    window.addEventListener('keydown', key, true)
    window.addEventListener('scroll', scroll, true)
    window.addEventListener('resize', place)
    return () => {
      window.removeEventListener('pointerdown', down, true)
      window.removeEventListener('keydown', key, true)
      window.removeEventListener('scroll', scroll, true)
      window.removeEventListener('resize', place)
    }
  })
</script>

{#if task}
  <div
    class="pop"
    role="dialog"
    aria-label="Checklist of {task.title}"
    tabindex="-1"
    bind:this={el}
    style:top="{pos?.top ?? 0}px"
    style:left="{pos?.left ?? 0}px"
    style:visibility={pos ? 'visible' : 'hidden'}
    {onpointerenter}
    {onpointerleave}
    onfocusout={(e) => {
      const to = e.relatedTarget as Node | null
      if (to && !el?.contains(to) && !anchor.contains(to)) onclose()
    }}>
    <div class="head">
      <strong class:done={task.status === 'done'}>{task.title}</strong>
      <span class="muted">{done}/{task.checklist.length}</span>
    </div>
    <div class="bar"><div style:width="{(done / task.checklist.length) * 100}%"></div></div>
    <ul>
      {#each task.checklist as it (it.id)}
        <li class:done={it.done}>
          <label>
            <input type="checkbox" checked={it.done} onchange={() => store.tickItem(task.id, it.id)} />
            <span>{it.text}</span>
          </label>
        </li>
      {/each}
    </ul>
    <button
      class="btn small open"
      onclick={() => {
        ui.editing = task.id
        onclose()
      }}>Open task</button>
  </div>
{/if}

<style>
  .pop {
    position: fixed;
    z-index: 60;
    width: min(280px, calc(100vw - 16px));
    max-height: min(420px, calc(100dvh - 16px));
    overflow-y: auto;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-lg);
    font-size: 14px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }
  .head strong {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .head strong.done {
    text-decoration: line-through;
  }
  .head span {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    height: 4px;
    margin: 6px 0 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 200ms;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  label {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 5px 0;
    cursor: pointer;
  }
  input {
    margin-top: 3px;
    flex: none;
  }
  li.done span {
    text-decoration: line-through;
    color: var(--muted);
  }
  .open {
    margin-top: 6px;
    width: 100%;
  }
</style>
