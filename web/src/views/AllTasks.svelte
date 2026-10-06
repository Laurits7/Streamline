<script lang="ts">
  // Every open task, across the inbox and all projects, in any view.
  import Icon from '../lib/components/Icon.svelte'
  import QuickAdd from '../lib/components/QuickAdd.svelte'
  import TaskRow from '../lib/components/TaskRow.svelte'
  import TaskViews from '../lib/components/TaskViews.svelte'
  import { dropList, type DragItem } from '../lib/dnd.svelte'
  import { keyAt } from '../lib/order'
  import { store } from '../lib/store.svelte'

  let q = $state('')
  const match = (title: string) => !q.trim() || title.toLowerCase().includes(q.trim().toLowerCase())
  const sections = $derived([
    { id: null as string | null, name: 'Inbox', color: null as string | null, tasks: store.tasksIn(null).filter((t) => match(t.title)) },
    ...store.projectTree().map(({ project: p }) => ({
      id: p.id as string | null,
      name: store.projectPath(p.id),
      color: p.color,
      tasks: store.tasksIn(p.id).filter((t) => match(t.title)),
    })),
  ])
  const total = $derived(sections.reduce((n, s) => n + s.tasks.length, 0))

  function dropInto(projectId: string | null, list: { id: string; position: string }[], item: DragItem, index: number) {
    if (item.kind !== 'task') return
    store.moveToProject(item.taskId, projectId, keyAt(list.filter((t) => t.id !== item.taskId).map((t) => t.position), index))
  }
</script>

<header class="head">
  <h1>All tasks</h1>
  <p class="muted">{total} open</p>
</header>

<TaskViews scope={{ kind: 'all' }}>
  {#snippet list()}
    <QuickAdd placeholder="Add to inbox…" onadd={(title) => store.createTask({ title })} />
    <input class="search" type="text" bind:value={q} placeholder="Filter tasks…" aria-label="Filter tasks" />
    {#each sections.filter((s) => s.tasks.length || s.id === null) as s (s.id ?? 'inbox')}
      <h2 class="section-title">
        {#if s.id}<i class="dot" style:background={s.color ?? 'var(--faint)'}></i>{:else}<Icon name="inbox" size={14} />{/if}
        <a href={s.id ? `/projects/${s.id}` : '/inbox'}>{s.name}</a>
        <span class="n">{s.tasks.length}</span>
      </h2>
      <div
        class="card list"
        use:dropList={{
          accepts: (it) => it.kind === 'task',
          drop: (it, i) => dropInto(s.id, s.tasks, it, i),
          keyMove: (id, i) => store.reorderTask(s.tasks, id, i),
        }}>
        {#each s.tasks as t (t.id)}
          <TaskRow task={t} handle planButton />
        {:else}
          <p class="empty">{q ? 'No matches.' : 'Nothing here.'}</p>
        {/each}
      </div>
    {/each}
  {/snippet}
</TaskViews>

<style>
  .head {
    margin-bottom: 12px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .head p {
    margin: 2px 0 0;
  }
  .search {
    margin-top: 10px;
  }
  .list {
    overflow: hidden;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .section-title a:hover {
    color: var(--accent);
  }
  .n {
    margin-left: auto;
    font-weight: 500;
  }
  .empty {
    padding: 16px;
    margin: 0;
  }
</style>
