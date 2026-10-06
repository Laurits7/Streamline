<script lang="ts">
  import Icon from '../lib/components/Icon.svelte'
  import ProjectTree from '../lib/components/ProjectTree.svelte'
  import QuickAdd from '../lib/components/QuickAdd.svelte'
  import { droppable, type DragItem } from '../lib/dnd.svelte'
  import { store } from '../lib/store.svelte'

  const hasProjects = $derived(store.childProjects(null).length > 0)
  // Archived subtrees are listed by their top-most archived project.
  const archived = $derived(
    store
      .projectList(true)
      .filter((p) => p.archived_at && !(p.parent_id && store.projects.get(p.parent_id)?.archived_at)),
  )
  const inboxCount = $derived([...store.tasks.values()].filter((t) => t.status === 'open' && !t.project_id).length)
  let showArchived = $state(false)

  const inboxTarget = {
    accepts: (it: DragItem) => it.kind === 'task',
    drop: (it: DragItem) => it.kind === 'task' && store.moveToProject(it.taskId, null),
  }
</script>

<header class="head">
  <h1>Projects</h1>
  <p class="muted">Drag a project onto another to make it a subproject.</p>
</header>

<QuickAdd placeholder="New project…" onadd={(name) => store.createProject(name)} />

<div class="card list">
  <a class="item drop-zone" href="/inbox" draggable="false" use:droppable={inboxTarget}>
    <span class="icon"><Icon name="inbox" /></span>
    <span class="name">Inbox</span>
    <span class="count">{inboxCount}</span>
  </a>
</div>

{#if hasProjects}
  <div class="card list tree"><ProjectTree /></div>
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
          <span class="name muted">{store.projectPath(p.id)}</span>
        </a>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .head {
    margin-bottom: 16px;
  }
  .head p {
    margin: 2px 0 0;
    font-size: 14px;
  }
  .tree :global(.level .level .item) {
    background: var(--surface);
  }
  .tree :global(.item:last-child) {
    border-bottom: 0;
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
