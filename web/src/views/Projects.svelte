<script lang="ts">
  import Icon from '../lib/components/Icon.svelte'
  import QuickAdd from '../lib/components/QuickAdd.svelte'
  import { sortable } from '../lib/sortable'
  import { store } from '../lib/store.svelte'

  const projects = $derived(store.projectList())
  const archived = $derived(store.projectList(true).filter((p) => p.archived_at))
  const counts = $derived.by(() => {
    const m = new Map<string | null, number>()
    for (const t of store.tasks.values()) if (t.status === 'open') m.set(t.project_id, (m.get(t.project_id) ?? 0) + 1)
    return m
  })
  let showArchived = $state(false)
</script>

<header class="head">
  <h1>Projects</h1>
</header>

<QuickAdd placeholder="New project…" onadd={(name) => store.createProject(name)} />

<div class="card list">
  <a class="item" href="/inbox">
    <span class="icon"><Icon name="inbox" /></span>
    <span class="name">Inbox</span>
    <span class="count">{counts.get(null) ?? 0}</span>
  </a>
</div>

{#if projects.length}
  <div class="card list" use:sortable={{ onMove: (id, i) => store.reorderProject(projects, id, i) }}>
    {#each projects as p (p.id)}
      <div class="item" data-id={p.id}>
        <button class="handle" data-handle aria-label="Reorder {p.name}"><Icon name="grip" size={16} /></button>
        <a class="link" href="/projects/{p.id}">
          <i class="dot" style:background={p.color ?? 'var(--faint)'}></i>
          <span class="name">{p.name}</span>
          <span class="count">{counts.get(p.id) ?? 0}</span>
        </a>
      </div>
    {/each}
  </div>
{:else}
  <p class="empty">No projects yet. Group related tasks into projects like “House”, “Work” or “Garden”.</p>
{/if}

{#if archived.length}
  <button class="section-title toggle" onclick={() => (showArchived = !showArchived)} aria-expanded={showArchived}>
    <Icon name="archive" size={14} /> Archived ({archived.length})
  </button>
  {#if showArchived}
    <div class="card list">
      {#each archived as p (p.id)}
        <a class="item" href="/projects/{p.id}">
          <i class="dot" style:background={p.color ?? 'var(--faint)'}></i>
          <span class="name muted">{p.name}</span>
        </a>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .head {
    margin-bottom: 16px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .list {
    overflow: hidden;
    margin-top: 12px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 52px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .item:last-child {
    border-bottom: 0;
  }
  .link {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 12px;
    align-self: stretch;
  }
  .handle {
    color: var(--faint);
    touch-action: none;
    cursor: grab;
    margin-left: -6px;
    display: grid;
    place-items: center;
    height: 36px;
  }
  .icon {
    color: var(--accent);
    display: grid;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex: none;
  }
  .name {
    flex: 1;
    font-weight: 550;
  }
  .count {
    color: var(--muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .toggle {
    width: 100%;
  }
</style>
