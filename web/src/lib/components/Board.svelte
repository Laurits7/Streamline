<script lang="ts">
  // Kanban board (SPEC §6.7): the same tasks in columns. Dropping a card in another
  // column changes the field the board is grouped by; within a column it reorders.
  import type { Task } from '../api/types/Task'
  import { announce } from '../announce.svelte'
  import { dropList, type DragItem } from '../dnd.svelte'
  import { keyAt } from '../order'
  import { store } from '../store.svelte'
  import { DIFFICULTY_COLUMNS, statusColumn, type BoardGroup } from '../views'
  import TaskRow from './TaskRow.svelte'

  let {
    tasks,
    group,
    projectColumns,
  }: {
    /** Open tasks plus recently completed ones of the scope. */
    tasks: Task[]
    group: BoardGroup
    /** For grouping by project: column project ids (null = inbox), each covering its subtree. */
    projectColumns: { id: string | null; title: string; color: string | null; own?: boolean }[]
  } = $props()

  type Column = { id: string; title: string; color?: string | null; tasks: Task[] }
  const byPosition = (a: Task, b: Task) => (a.position < b.position ? -1 : a.position > b.position ? 1 : 0)

  const columns = $derived.by((): Column[] => {
    const open = tasks.filter((t) => t.status === 'open')
    if (group === 'status')
      return [
        { id: 'todo', title: 'To do', tasks: open.filter((t) => statusColumn(t) === 'todo').sort(byPosition) },
        { id: 'doing', title: 'In progress', tasks: open.filter((t) => statusColumn(t) === 'doing').sort(byPosition) },
        {
          id: 'done',
          title: 'Done (last 2 weeks)',
          tasks: tasks.filter((t) => t.status === 'done').sort((a, b) => ((a.completed_at ?? '') < (b.completed_at ?? '') ? 1 : -1)),
        },
      ]
    if (group === 'difficulty')
      return DIFFICULTY_COLUMNS.map((c) => ({ ...c, tasks: open.filter((t) => String(t.difficulty ?? 0) === c.id).sort(byPosition) }))
    if (group === 'type')
      return [...store.taskTypes.values()].map((tt) => ({
        id: tt.id,
        title: tt.name,
        tasks: open.filter((t) => t.task_type_id === tt.id).sort(byPosition),
      }))
    return projectColumns.map((c) => {
      const ids = c.id === null ? null : new Set(c.own ? [c.id] : store.subtree(c.id))
      return {
        id: c.id ?? '',
        title: c.title,
        color: c.color,
        tasks: open
          .filter((t) =>
            ids === null ? t.project_id === null : (t.project_id !== null && ids.has(t.project_id)) || t.also_project_ids.some((p) => ids.has(p)),
          )
          .sort(byPosition),
      }
    })
  })

  function drop(col: Column, item: DragItem, index: number) {
    if (item.kind !== 'task') return
    const t = store.tasks.get(item.taskId)
    if (!t) return
    const position = keyAt(col.tasks.filter((x) => x.id !== t.id).map((x) => x.position), index)
    const already = col.tasks.some((x) => x.id === t.id)
    if (group === 'status') {
      if (col.id === 'done') store.updateTask(t.id, { status: 'done' })
      else {
        store.updateTask(t.id, t.status === 'open' ? { position } : { status: 'open', position })
        if ((col.id === 'doing') !== !!t.started_at || t.status !== 'open') store.setInProgress(t.id, col.id === 'doing')
      }
    } else if (group === 'difficulty') store.updateTask(t.id, { difficulty: col.id === '0' ? null : Number(col.id), position })
    else if (group === 'type') store.updateTask(t.id, { task_type_id: col.id, position })
    else if (!already) store.moveToProject(t.id, col.id || null, position)
    else if (t.project_id === (col.id || null)) store.updateTask(t.id, { position })
    if (!already) announce(`Moved “${t.title}” to ${col.title}`)
  }
</script>

<div class="board" role="list" aria-label="Board" data-autoscroll-x>
  {#each columns as col (col.id)}
    <section class="col" role="listitem" aria-label="{col.title}, {col.tasks.length} tasks">
      <header>
        {#if col.color !== undefined}<i style:background={col.color ?? 'var(--faint)'}></i>{/if}
        <h3>{col.title}</h3>
        <span class="count">{col.tasks.length}</span>
      </header>
      <div class="card list" use:dropList={{ accepts: (it) => it.kind === 'task', drop: (it, i) => drop(col, it, i) }}>
        {#each col.tasks as t (t.id)}
          <TaskRow task={t} showProject={group !== 'project'} dragClosed={group === 'status'} />
        {:else}
          <p class="empty">Drop tasks here</p>
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .board {
    display: flex;
    gap: 14px;
    overflow-x: auto;
    padding-bottom: 12px;
    scroll-snap-type: x proximity;
    margin-right: -16px;
    padding-right: 16px;
  }
  .col {
    flex: 1 0 min(240px, 82vw);
    max-width: 340px;
    scroll-snap-align: start;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 4px 8px;
  }
  header i {
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  h3 {
    font-size: 13px;
    font-weight: 700;
    flex: 1;
  }
  .count {
    font-size: 12px;
    color: var(--muted);
    background: var(--surface-2);
    border-radius: 999px;
    padding: 0 8px;
  }
  .list {
    overflow: hidden;
    min-height: 60px;
  }
  .empty {
    padding: 18px;
    margin: 0;
    font-size: 13px;
  }
</style>
