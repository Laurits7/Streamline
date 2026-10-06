<script lang="ts">
  // Inbox (projectId = null) or a single project's todo list.
  import Icon from '../lib/components/Icon.svelte'
  import QuickAdd from '../lib/components/QuickAdd.svelte'
  import TaskRow from '../lib/components/TaskRow.svelte'
  import { router } from '../lib/router.svelte'
  import { sortable } from '../lib/sortable'
  import { store } from '../lib/store.svelte'

  let { projectId = null }: { projectId?: string | null } = $props()

  const project = $derived(projectId ? store.projects.get(projectId) : null)
  const open = $derived(store.tasksIn(projectId))
  const closed = $derived(store.tasksIn(projectId, 'closed'))
  let showDone = $state(false)
  let menu = $state(false)

  function rename() {
    const name = prompt('Project name', project?.name)?.trim()
    if (name && projectId) store.updateProject(projectId, { name })
    menu = false
  }
  function archive() {
    if (!projectId || !project) return
    store.updateProject(projectId, { archived: !project.archived_at })
    menu = false
  }
  function remove() {
    if (!projectId || !project) return
    if (confirm(`Delete “${project.name}” and all its ${open.length + closed.length} tasks?`)) {
      store.deleteProject(projectId)
      router.go('/projects', true)
    }
  }
  const colors = ['#4f46e5', '#0891b2', '#16a34a', '#ca8a04', '#ea580c', '#dc2626', '#db2777', '#7c3aed', '#64748b']
</script>

{#if projectId && !project}
  <p class="empty">{store.ready ? 'Project not found.' : ''}</p>
{:else}
  <header class="head">
    <div>
      <h1>
        {#if project}<i class="dot" style:background={project.color ?? 'var(--faint)'}></i>{project.name}{:else}Inbox{/if}
      </h1>
      <p class="muted">
        {open.length} open{#if project?.archived_at} · archived{/if}
      </p>
    </div>
    {#if project}
      <div class="menu-wrap">
        <button class="icon-btn" onclick={() => (menu = !menu)} aria-label="Project actions" aria-expanded={menu}><Icon name="more" /></button>
        {#if menu}
          <div class="menu card" role="menu">
            <button role="menuitem" onclick={rename}><Icon name="edit" size={16} /> Rename</button>
            <div class="colors">
              {#each colors as c (c)}
                <button class="swatch" style:background={c} aria-label="Color {c}" onclick={() => { store.updateProject(projectId!, { color: c }); menu = false }}></button>
              {/each}
            </div>
            <button role="menuitem" onclick={archive}><Icon name="archive" size={16} /> {project.archived_at ? 'Unarchive' : 'Archive'}</button>
            <button role="menuitem" class="danger" onclick={remove}><Icon name="trash" size={16} /> Delete</button>
          </div>
        {/if}
      </div>
    {/if}
  </header>

  <QuickAdd placeholder={project ? `Add to ${project.name}…` : 'Add to inbox…'} onadd={(title) => store.createTask({ title, project_id: projectId })} />

  {#if open.length}
    <div class="card list" use:sortable={{ onMove: (id, index) => store.reorderTask(open, id, index) }}>
      {#each open as t (t.id)}
        <TaskRow task={t} draggable planButton />
      {/each}
    </div>
  {:else}
    <p class="empty">{project ? 'No open tasks in this project.' : 'Inbox zero. Capture anything above.'}</p>
  {/if}

  {#if closed.length}
    <button class="section-title toggle" onclick={() => (showDone = !showDone)} aria-expanded={showDone}>
      <Icon name={showDone ? 'left' : 'right'} size={14} /> Completed ({closed.length})
    </button>
    {#if showDone}
      <div class="card list">
        {#each closed.slice(0, 100) as t (t.id)}
          <TaskRow task={t} />
        {/each}
      </div>
    {/if}
  {/if}
{/if}

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 16px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }
  .head p {
    margin: 2px 0 0;
  }
  .list {
    overflow: hidden;
    margin-top: 12px;
  }
  .toggle {
    width: 100%;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: 40px;
    z-index: 20;
    min-width: 200px;
    padding: 6px;
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
  }
  .menu > button {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 10px;
    border-radius: 8px;
    text-align: left;
  }
  .menu > button:hover {
    background: var(--surface-2);
  }
  .menu .danger {
    color: var(--danger);
  }
  .colors {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 6px 10px;
  }
  .swatch {
    width: 20px;
    height: 20px;
    border-radius: 50%;
  }
</style>
