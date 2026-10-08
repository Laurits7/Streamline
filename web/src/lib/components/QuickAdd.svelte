<script lang="ts">
  import { store } from '../store.svelte'
  import Icon from './Icon.svelte'
  let {
    placeholder,
    onadd,
    ondefault,
  }: {
    placeholder: string
    onadd: (title: string) => void
    /** Offer default tasks (D-71): matching ones while typing, and a list button. */
    ondefault?: (templateId: string) => void
  } = $props()
  let value = $state('')
  let listOpen = $state(false)
  const matches = $derived(ondefault ? store.matchDefaults(value) : [])
  const all = $derived(ondefault ? store.defaultTasks() : [])

  function submit(e: Event) {
    e.preventDefault()
    const t = value.trim()
    if (!t) return
    onadd(t)
    value = ''
  }
  function pick(id: string) {
    ondefault?.(id)
    value = ''
    listOpen = false
  }
  const steps = (n: number) => (n ? ` · ${n} step${n === 1 ? '' : 's'}` : '')
</script>

<div class="wrap" data-tour="quickadd">
  <form class="quick" onsubmit={submit}>
    <Icon name="plus" />
    <input
      type="text"
      bind:value
      {placeholder}
      aria-label={placeholder}
      enterkeyhint="done"
      onkeydown={(e) => {
        if (e.key === 'Escape') listOpen = false
      }} />
    {#if value.trim()}
      <button class="btn primary small" type="submit">Add</button>
    {:else if all.length}
      <button
        class="icon-btn"
        type="button"
        aria-expanded={listOpen}
        aria-label="Add a default task"
        title="Default tasks"
        onclick={() => (listOpen = !listOpen)}><Icon name="checklist" size={18} /></button>
    {/if}
  </form>
  {#if (value.trim() && matches.length) || (listOpen && !value.trim())}
    <ul class="suggest card" aria-label="Default tasks">
      {#each value.trim() ? matches : all as t (t.id)}
        <li>
          <button type="button" onclick={() => pick(t.id)}>
            <Icon name="checklist" size={14} />
            <span class="t">{t.title}</span>
            <span class="muted">{steps(t.checklist.length)}</span>
          </button>
        </li>
      {/each}
    </ul>
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
  .suggest button:focus-visible {
    background: var(--accent-soft);
  }
  .t {
    color: var(--text);
    font-weight: 550;
  }
  .muted {
    font-size: 12px;
  }
</style>
