<script lang="ts">
  // Eisenhower matrix (SPEC §6.7, D-22). Dragging between quadrants changes importance
  // and urgency only where they cross the threshold, so finer scores are kept.
  import type { Task } from '../api/types/Task'
  import { announce } from '../announce.svelte'
  import { droppable, type DragItem } from '../dnd.svelte'
  import { store } from '../store.svelte'
  import { matrixPatch, QUADRANTS, quadrantOf, type Quadrant } from '../views'
  import TaskRow from './TaskRow.svelte'

  let { tasks }: { tasks: Task[] } = $props()

  const open = $derived(tasks.filter((t) => t.status === 'open'))
  const sorted = (ts: Task[]) =>
    [...ts].sort((a, b) => (a.due_date ?? '9999') .localeCompare(b.due_date ?? '9999') || (a.position < b.position ? -1 : 1))

  function drop(q: Quadrant, item: DragItem) {
    if (item.kind !== 'task') return
    const t = store.tasks.get(item.taskId)
    if (!t) return
    const patch = matrixPatch(t, q)
    if (Object.keys(patch).length) {
      store.updateTask(t.id, patch)
      announce(`“${t.title}” is now in ${QUADRANTS.find((x) => x.id === q)!.title}`)
    }
  }
</script>

<div class="matrix">
  <div class="axis top" aria-hidden="true"><span>Urgent</span><span>Not urgent</span></div>
  {#each QUADRANTS as q (q.id)}
    {@const items = sorted(open.filter((t) => quadrantOf(t) === q.id))}
    <section class="quad {q.id}" aria-label="{q.title}: {q.hint}, {items.length} tasks">
      <header>
        <h3>{q.title}</h3>
        <span class="hint">{q.hint}</span>
        <span class="count">{items.length}</span>
      </header>
      <div class="card list drop-zone" use:droppable={{ accepts: (it) => it.kind === 'task', drop: (it) => drop(q.id, it) }}>
        {#each items as t (t.id)}
          <TaskRow task={t} showProject />
        {:else}
          <p class="empty">Drop tasks here</p>
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .matrix {
    display: grid;
    grid-template-columns: 1fr;
    gap: 14px;
  }
  .axis {
    display: none;
  }
  @media (min-width: 900px) {
    .matrix {
      grid-template-columns: 1fr 1fr;
      grid-template-areas: 'axis axis' 'do schedule' 'delegate eliminate';
    }
    .axis {
      grid-area: axis;
      display: grid;
      grid-template-columns: 1fr 1fr;
      text-align: center;
      font-size: 12px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      color: var(--faint);
    }
    /* Columns: urgent | not urgent; rows: important | not important */
    .do {
      grid-area: do;
    }
    .schedule {
      grid-area: schedule;
    }
    .delegate {
      grid-area: delegate;
    }
    .eliminate {
      grid-area: eliminate;
    }
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 0 4px 8px;
  }
  h3 {
    font-size: 14px;
    font-weight: 700;
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
    flex: 1;
  }
  .count {
    font-size: 12px;
    color: var(--muted);
  }
  .do h3 {
    color: var(--danger);
  }
  .schedule h3 {
    color: var(--accent);
  }
  .delegate h3 {
    color: var(--warn);
  }
  .eliminate h3 {
    color: var(--muted);
  }
  .list {
    overflow: hidden;
    min-height: 80px;
  }
  .empty {
    padding: 22px;
    margin: 0;
    font-size: 13px;
  }
</style>
