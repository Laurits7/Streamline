<script lang="ts">
  // "Add a task…" with default tasks (D-71, D-86): the list opens when the field gets focus
  // and narrows as you type; arrows and Enter pick one, Enter with none highlighted adds
  // the typed text. A default task with kinds asks which kinds to add (one task each).
  import type { TaskTemplate } from '../api/types/TaskTemplate'
  import type { DefaultMatch } from '../defaults'
  import { store } from '../store.svelte'
  import Icon from './Icon.svelte'
  let {
    placeholder,
    onadd,
    ondefault,
  }: {
    placeholder: string
    onadd: (title: string) => void
    /** Add from a default task, one task per chosen kind (none = the default task itself). */
    ondefault?: (templateId: string, kindIds: string[]) => void
  } = $props()
  const uid = Math.random().toString(36).slice(2, 8)
  let value = $state('')
  let open = $state(false)
  let active = $state(-1)
  let choosing = $state<{ tpl: TaskTemplate; kinds: string[] } | null>(null)
  let wrap: HTMLDivElement
  let input: HTMLInputElement
  const list = $derived<DefaultMatch[]>(
    !ondefault ? [] : value.trim() ? store.matchDefaults(value) : store.defaultTasks().map((tpl) => ({ tpl, kind: null })),
  )
  const shown = $derived(open && !choosing && list.length > 0)
  $effect(() => {
    void list
    active = -1
  })

  function close() {
    open = false
    choosing = null
    active = -1
  }
  function submit(e: Event) {
    e.preventDefault()
    if (shown && active >= 0) return pick(list[active])
    const t = value.trim()
    if (!t) return
    onadd(t)
    value = ''
    close()
  }
  function pick(m: DefaultMatch) {
    if (m.tpl.kinds.length) {
      choosing = { tpl: m.tpl, kinds: m.kind ? [m.kind.id] : [] }
      return
    }
    add(m.tpl.id, [])
  }
  function add(id: string, kinds: string[]) {
    ondefault?.(id, kinds)
    value = ''
    close()
    input?.focus()
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (choosing) choosing = null
      else close()
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      if (!list.length) return
      e.preventDefault()
      open = true
      const n = list.length
      active = e.key === 'ArrowDown' ? (active + 1) % n : (active - 1 + n) % n
    }
  }
  const steps = (n: number) => (n ? ` · ${n} step${n === 1 ? '' : 's'}` : '')
  const kindsLabel = (t: TaskTemplate) => (t.kinds.length ? ` · ${t.kinds.map((k) => k.name).join(', ')}` : '')
</script>

<div
  class="wrap"
  data-tour="quickadd"
  bind:this={wrap}
  onfocusout={(e) => {
    const to = e.relatedTarget as Node | null
    if (!to || !wrap.contains(to)) close()
  }}>
  <form class="quick" onsubmit={submit}>
    <Icon name="plus" />
    <input
      type="text"
      bind:this={input}
      bind:value
      {placeholder}
      aria-label={placeholder}
      enterkeyhint="done"
      role="combobox"
      aria-expanded={shown}
      aria-controls="qa-{uid}"
      aria-autocomplete="list"
      aria-activedescendant={shown && active >= 0 ? `qa-${uid}-${active}` : undefined}
      onfocus={() => (open = true)}
      onclick={() => (open = true)}
      oninput={() => (open = true)}
      onkeydown={key} />
    {#if value.trim()}
      <button class="btn primary small" type="submit">Add</button>
    {:else if ondefault && store.taskTemplates.size}
      <button
        class="icon-btn"
        type="button"
        aria-expanded={shown}
        aria-label="Add a default task"
        title="Default tasks"
        onclick={() => (open && !choosing ? close() : ((choosing = null), (open = true)))}><Icon name="checklist" size={18} /></button>
    {/if}
  </form>
  {#if shown}
    <ul class="suggest card" id="qa-{uid}" role="listbox" aria-label="Default tasks">
      {#each list as m, i (m.tpl.id + (m.kind?.id ?? ''))}
        <li role="option" id="qa-{uid}-{i}" aria-selected={i === active} class:active={i === active}>
          <button type="button" tabindex="-1" onpointerdown={(e) => e.preventDefault()} onclick={() => pick(m)}>
            <Icon name="checklist" size={14} />
            {#if m.kind}
              <span class="t">{m.tpl.title} › {m.kind.name}</span>
            {:else}
              <span class="t">{m.tpl.title}</span>
              <span class="muted">{m.tpl.kinds.length ? kindsLabel(m.tpl) : steps(m.tpl.checklist.length)}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {:else if choosing}
    {@const c = choosing}
    <div class="suggest card kinds" role="group" aria-label="Which kinds of {c.tpl.title}?">
      <div class="khead">
        <button type="button" class="icon-btn" aria-label="Back" onclick={() => (choosing = null)}><Icon name="left" size={16} /></button>
        <strong>{c.tpl.title}</strong>
        <span class="muted">one task per kind</span>
      </div>
      {#each c.tpl.kinds as k (k.id)}
        <label class="kind">
          <input
            type="checkbox"
            checked={c.kinds.includes(k.id)}
            onchange={(e) => {
              const on = (e.currentTarget as HTMLInputElement).checked
              c.kinds = on ? [...c.kinds, k.id] : c.kinds.filter((x) => x !== k.id)
            }} />
          <span>{k.name}</span>
          <span class="muted">{steps(k.checklist.length || c.tpl.checklist.length)}</span>
        </label>
      {/each}
      <div class="kacts">
        <button type="button" class="btn small" onclick={() => (c.kinds = c.kinds.length === c.tpl.kinds.length ? [] : c.tpl.kinds.map((k) => k.id))}>
          {c.kinds.length === c.tpl.kinds.length ? 'None' : 'All'}
        </button>
        <button type="button" class="btn primary small" onclick={() => add(c.tpl.id, c.tpl.kinds.filter((k) => c.kinds.includes(k.id)).map((k) => k.id))}>
          {c.kinds.length ? `Add ${c.kinds.length} task${c.kinds.length === 1 ? '' : 's'}` : `Add ${c.tpl.title}`}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .quick {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 4px 6px 4px 14px;
    color: var(--muted);
    transition: border-color 120ms, box-shadow 120ms;
  }
  .quick:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  input {
    border: 0 !important;
    background: transparent !important;
    padding: 10px 0 !important;
    color: var(--text);
    box-shadow: none !important;
  }
  .suggest {
    list-style: none;
    margin: 4px 0 0;
    padding: 4px;
    position: absolute;
    left: 0;
    right: 0;
    z-index: 20;
    box-shadow: var(--shadow);
    max-height: 280px;
    overflow-y: auto;
  }
  .suggest button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--accent);
  }
  .suggest button:hover,
  .suggest button:focus-visible,
  .suggest li.active button {
    background: var(--accent-soft);
  }
  .kinds {
    padding: 6px 10px 10px;
  }
  .khead {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
  }
  .khead .muted {
    margin-left: auto;
  }
  .kinds .khead button {
    width: auto;
    padding: 4px;
    color: var(--muted);
  }
  .kind {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 4px;
    cursor: pointer;
  }
  .kind input {
    flex: none;
  }
  .kacts {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin-top: 6px;
  }
  .kinds .kacts button {
    width: auto;
    padding: 6px 12px;
  }
  .kinds .kacts .primary {
    color: var(--accent-text);
  }
  .t {
    color: var(--text);
    font-weight: 550;
  }
  .muted {
    font-size: 12px;
  }
</style>
