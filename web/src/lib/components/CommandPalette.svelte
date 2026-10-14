<script lang="ts">
  // Search and commands (10.2): Ctrl+K (⌘K) or "/". Enter opens the chosen item; Tab shows
  // what can be done with a task (complete, plan, focus…). Typing something new offers to
  // add it to the inbox.
  import { tick } from 'svelte'
  import { search, snippet, fold } from '../palette'
  import { paletteItems, type Entry } from '../paletteItems'
  import { router } from '../router.svelte'
  import { store } from '../store.svelte'
  import { toast } from '../toast.svelte'
  import { ui } from '../ui.svelte'
  import { addDays } from '../dates'
  import Icon from './Icon.svelte'

  let query = $state('')
  let active = $state(0)
  /** The task whose actions are shown (Tab), or null. */
  let acting = $state<Entry | null>(null)
  let actionIndex = $state(0)
  let input = $state<HTMLInputElement>()

  const close = () => (ui.palette = false)
  const go = (href: string) => {
    close()
    router.go(href)
  }

  const commands: Entry[] = [
    { key: 'action:focus', kind: 'action', title: 'Start focusing (no task)', run: () => (store.focus('start', null), go('/focus')) },
    { key: 'action:tomorrow', kind: 'action', title: 'Plan tomorrow', run: () => go(`/plan/${addDays(store.today, 1)}`) },
    { key: 'action:shortcuts', kind: 'action', title: 'Keyboard shortcuts', run: () => (close(), (ui.shortcuts = true)) },
    { key: 'action:tour', kind: 'action', title: 'Take the tour', run: () => (close(), (ui.tour = 0)) },
  ]

  const items = $derived(paletteItems(commands))
  const results = $derived.by(() => {
    const found = search(items, query, 40)
    const q = query.trim()
    if (!q) return found
    const add: Entry = {
      key: 'action:add',
      kind: 'action',
      title: `Add “${q}” to the inbox`,
      run: () => {
        store.createTask({ title: q })
        toast(`Added “${q}” to the inbox`)
        close()
      },
    }
    // First when nothing matches well, else last.
    return found.length ? [...found, add] : [add]
  })
  $effect(() => {
    void query
    active = 0
    acting = null
  })
  $effect(() => {
    tick().then(() => input?.focus())
  })

  type TaskAction = { label: string; run: () => void }
  function taskActions(e: Entry): TaskAction[] {
    const t = e.taskId ? store.tasks.get(e.taskId) : undefined
    if (!t) return []
    const list: TaskAction[] = [{ label: 'Open', run: () => (close(), (ui.editing = t.id)) }]
    if (t.status === 'open' || t.status === 'done')
      list.push({ label: t.status === 'done' ? 'Reopen' : 'Complete', run: () => (store.toggleDone(t.id), close()) })
    if (t.status === 'open') {
      list.push({ label: 'Plan for today', run: () => (store.plan(t.id, store.today), toast(`“${t.title}” planned for today`), close()) })
      list.push({ label: 'Plan for tomorrow', run: () => (store.plan(t.id, addDays(store.today, 1)), toast(`“${t.title}” planned for tomorrow`), close()) })
      list.push({ label: 'Focus on it', run: () => (store.focus('start', t.id), go('/focus')) })
    }
    if (t.project_id) list.push({ label: 'Go to its project', run: () => go(`/projects/${t.project_id}`) })
    return list
  }

  function run(e: Entry) {
    if (e.run) e.run()
    else if (e.href) go(e.href)
    else if (e.taskId) {
      close()
      ui.editing = e.taskId
    }
  }

  function key(ev: KeyboardEvent) {
    const list = acting ? taskActions(acting) : results
    if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
      ev.preventDefault()
      const n = list.length
      if (!n) return
      const by = ev.key === 'ArrowDown' ? 1 : -1
      if (acting) actionIndex = (actionIndex + by + n) % n
      else active = (active + by + n) % n
      tick().then(() => document.querySelector('.palette [aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }))
    } else if (ev.key === 'Enter') {
      ev.preventDefault()
      if (acting) taskActions(acting)[actionIndex]?.run()
      else if (results[active]) run(results[active])
    } else if (ev.key === 'Tab') {
      ev.preventDefault()
      const cur = results[active]
      if (acting || ev.shiftKey) acting = null
      else if (cur?.taskId) {
        acting = cur
        actionIndex = 0
      }
    } else if (ev.key === 'Escape') {
      ev.preventDefault()
      ev.stopPropagation()
      if (acting) acting = null
      else close()
    }
  }

  const ICON: Record<string, string> = { action: 'bolt', page: 'right', task: 'check', project: 'folder', routine: 'repeat', journal: 'book' }
  const matchedText = (e: Entry) => {
    const q = query.trim()
    if (!q || !e.text) return null
    const inTitle = fold(q).split(/\s+/).every((w) => fold(e.title).includes(w))
    return inTitle ? null : snippet(e.text, q)
  }
</script>

<div class="backdrop" onclick={close} aria-hidden="true"></div>
<div class="palette card" role="dialog" aria-modal="true" aria-label="Search and commands">
  <div class="field">
    <Icon name="search" size={18} />
    <input
      bind:this={input}
      bind:value={query}
      type="text"
      placeholder="Search tasks, projects, journal… or type a command"
      aria-label="Search and commands"
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-activedescendant={acting ? undefined : results[active] ? `pal-${active}` : undefined}
      autocomplete="off"
      spellcheck="false"
      onkeydown={key} />
    <kbd>Esc</kbd>
  </div>
  {#if acting}
    <div class="acting">
      <button class="back" onclick={() => (acting = null)}><Icon name="left" size={14} /> {acting.title}</button>
      <ul role="listbox" aria-label="Actions">
        {#each taskActions(acting) as a, i (a.label)}
          <li role="option" aria-selected={i === actionIndex}>
            <button onpointermove={() => (actionIndex = i)} onclick={a.run}>{a.label}</button>
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <ul id="palette-list" role="listbox" aria-label="Results">
      {#each results as e, i (e.key)}
        {@const why = matchedText(e)}
        <li id="pal-{i}" role="option" aria-selected={i === active} class:closed={e.closed}>
          <button class="item" onpointermove={() => (active = i)} onclick={() => run(e)}>
            <span class="ic"><Icon name={ICON[e.kind]} size={15} /></span>
            <span class="txt">
              <span class="t">{e.title}</span>
              {#if why}<span class="sub">{why}</span>{:else if e.sub}<span class="sub">{e.sub}</span>{/if}
            </span>
          </button>
          {#if e.taskId}
            <button
              class="more"
              tabindex="-1"
              aria-label="Actions for {e.title}"
              title="Actions (Tab)"
              onclick={() => ((acting = e), (actionIndex = 0), input?.focus())}>⋯</button>
          {/if}
        </li>
      {:else}
        <li class="none muted">Nothing found</li>
      {/each}
    </ul>
  {/if}
  <p class="hint muted">↑↓ choose · Enter open · Tab actions on a task · ? all shortcuts</p>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    z-index: 80;
  }
  .palette {
    position: fixed;
    z-index: 81;
    top: min(12vh, 120px);
    left: 50%;
    transform: translateX(-50%);
    width: min(640px, calc(100vw - 24px));
    max-height: min(70vh, 560px);
    display: flex;
    flex-direction: column;
    padding: 0;
    overflow: hidden;
    box-shadow: var(--shadow-lg);
  }
  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
  }
  .field input {
    flex: 1;
    border: 0 !important;
    background: transparent !important;
    box-shadow: none !important;
    padding: 12px 0 !important;
    font-size: 16px;
    color: var(--text);
  }
  kbd {
    font: inherit;
    font-size: 11px;
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 6px;
    overflow-y: auto;
  }
  li[role='option'] {
    display: flex;
    align-items: center;
    border-radius: var(--radius-sm);
  }
  li .item,
  .acting li button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
  }
  li[aria-selected='true'] {
    background: var(--accent-soft);
  }
  .more {
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    color: var(--muted);
    font-size: 18px;
    opacity: 0;
  }
  li[aria-selected='true'] .more,
  li:hover .more {
    opacity: 1;
  }
  /* Touch screens: the actions button always shows, key hints don't. */
  @media (hover: none) {
    .more {
      opacity: 1;
    }
    kbd,
    .hint {
      display: none;
    }
  }
  li.closed .t {
    color: var(--muted);
    text-decoration: line-through;
  }
  .ic {
    color: var(--muted);
    display: grid;
  }
  .txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .t,
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    font-size: 12px;
    color: var(--muted);
  }
  .none {
    padding: 12px;
  }
  .acting {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .back {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px 0;
    font-weight: 600;
    text-align: left;
  }
  .hint {
    margin: 0;
    padding: 6px 14px 8px;
    font-size: 12px;
    border-top: 1px solid var(--border);
  }
</style>
