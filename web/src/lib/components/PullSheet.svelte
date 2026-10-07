<script lang="ts">
  // The ready stack: open tasks that can be pulled into a day.
  import type { Project } from '../api/types/Project'
  import ProjectMark from './ProjectMark.svelte'
  import { fmtMinutes, shortDate } from '../dates'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'
  import Icon from './Icon.svelte'
  import Sheet from './Sheet.svelte'

  let { date }: { date: string } = $props()
  let q = $state('')

  const groups = $derived.by(() => {
    const needle = q.trim().toLowerCase()
    const tasks = store.readyStack(date).filter((t) => !needle || t.title.toLowerCase().includes(needle))
    const out: { name: string; project: Project | null; tasks: typeof tasks }[] = []
    const inbox = tasks.filter((t) => !t.project_id)
    if (inbox.length) out.push({ name: 'Inbox', project: null, tasks: inbox })
    for (const { project: p } of store.projectTree()) {
      const ts = tasks.filter((t) => t.project_id === p.id)
      if (ts.length) out.push({ name: store.projectPath(p.id), project: p, tasks: ts })
    }
    return out
  })
  const planned = $derived(store.dayEntries(date).length)
  const elsewhere = $derived(
    store.currentPlace
      ? [...store.tasks.values()].filter((t) => t.status === 'open' && !store.atCurrentPlace(t)).length
      : 0,
  )
</script>

<Sheet title="Pull into {shortDate(date, store.today)}" onclose={() => (ui.pullFor = null)}>
  <input type="text" bind:value={q} placeholder="Search tasks…" aria-label="Search tasks" />
  <p class="muted count">
    {planned} planned for {shortDate(date, store.today).toLowerCase()}
    {#if elsewhere}· {elsewhere} at other places hidden{/if}
  </p>
  {#each groups as g (g.name)}
    <h3><ProjectMark project={g.project} />{g.name}</h3>
    <div class="card list">
      {#each g.tasks as t (t.id)}
        {@const other = store.entryForTask(t.id)}
        <button class="item" onclick={() => store.plan(t.id, date)}>
          <span class="add"><Icon name="plus" size={16} /></span>
          <span class="title">{t.title}</span>
          <span class="meta">
            {#if other}<span class="muted">{shortDate(other.date, store.today)}</span>{/if}
            {#if t.estimate_min}<span>{fmtMinutes(t.estimate_min)}</span>{/if}
            {#if t.due_date}<span class:overdue={t.due_date <= date}>due {shortDate(t.due_date, store.today)}</span>{/if}
          </span>
        </button>
      {/each}
    </div>
  {:else}
    <p class="empty">{q ? 'No matching tasks.' : 'Everything open is already planned. Nice.'}</p>
  {/each}
</Sheet>

<style>
  .count {
    font-size: 13px;
    margin: 8px 2px 0;
  }
  h3 {
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
    margin: 18px 2px 8px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .list {
    overflow: hidden;
  }
  .item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 10px;
    padding: 12px;
    text-align: left;
    border-bottom: 1px solid var(--border);
    transition: background 100ms;
  }
  .item:last-child {
    border-bottom: 0;
  }
  .item:hover {
    background: var(--surface-2);
  }
  .add {
    color: var(--accent);
    display: grid;
    place-items: center;
  }
  .title {
    flex: 1;
    min-width: 0;
  }
  .meta {
    display: flex;
    gap: 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .overdue {
    color: var(--danger);
  }
</style>
