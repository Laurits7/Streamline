<script lang="ts">
  // Nested project list. Drag a project onto the middle of another to nest it,
  // or between rows to reorder; drop a task on a row to move the task there.
  import { draggable, droppable, dropList, type DragItem, type DropAt } from '../dnd.svelte'
  import { store } from '../store.svelte'
  import { isCollapsed, toggleCollapsed } from '../ui.svelte'
  import Icon from './Icon.svelte'
  import ProjectTree from './ProjectTree.svelte'

  let { parentId = null, depth = 0 }: { parentId?: string | null; depth?: number } = $props()
  const projects = $derived(store.childProjects(parentId))

  /** Middle half of a row = "put inside"; top/bottom quarters fall through to reordering. */
  const inMiddle = (at: DropAt) => {
    const r = at.el.getBoundingClientRect()
    return at.y > r.top + r.height * 0.25 && at.y < r.bottom - r.height * 0.25
  }
  const rowTarget = (id: string) => ({
    accepts: (it: DragItem, at: DropAt) =>
      it.kind === 'task' || (it.kind === 'project' && it.projectId !== id && store.canMoveProject(it.projectId, id) && inMiddle(at)),
    drop: (it: DragItem) => {
      if (it.kind === 'task') store.moveToProject(it.taskId, id)
      else {
        store.moveProject(it.projectId, id)
        if (isCollapsed(id)) toggleCollapsed(id)
      }
    },
  })
</script>

<div
  class="level"
  use:dropList={{
    accepts: (it) => it.kind === 'project' && store.canMoveProject(it.projectId, parentId),
    drop: (it, i) => it.kind === 'project' && store.moveProject(it.projectId, parentId, i),
    keyMove: (id, i) => store.reorderProject(projects, id, i),
  }}>
  {#each projects as p (p.id)}
    {@const kids = store.childProjects(p.id).length}
    <div
      class="item drop-zone"
      style:padding-left="{8 + depth * 22}px"
      data-id={p.id}
      use:draggable={{ item: () => ({ kind: 'project', projectId: p.id }) }}
      use:droppable={rowTarget(p.id)}>
      <button class="handle" data-handle aria-label="Reorder {p.name} (drag, or use arrow keys)"><Icon name="grip" size={16} /></button>
      {#if kids}
        <button
          class="twisty"
          data-nodrag
          aria-label="{isCollapsed(p.id) ? 'Expand' : 'Collapse'} {p.name}"
          aria-expanded={!isCollapsed(p.id)}
          onclick={() => toggleCollapsed(p.id)}>
          <span class:open={!isCollapsed(p.id)}><Icon name="right" size={14} /></span>
        </button>
      {:else}
        <span class="twisty"></span>
      {/if}
      <a class="link" href="/projects/{p.id}" draggable="false">
        <i class="dot" style:background={p.color ?? 'var(--faint)'}></i>
        <span class="name">{p.name}</span>
        {#if kids}<span class="kids muted">{kids} sub</span>{/if}
        <span class="count">{store.openCountDeep(p.id)}</span>
      </a>
    </div>
    {#if kids && !isCollapsed(p.id)}
      <ProjectTree parentId={p.id} depth={depth + 1} />
    {/if}
  {/each}
</div>

<style>
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 50px;
    padding-right: 14px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .handle {
    color: var(--faint);
    touch-action: none;
    cursor: grab;
    display: grid;
    place-items: center;
    height: 36px;
    width: 20px;
  }
  .twisty {
    width: 22px;
    height: 28px;
    display: grid;
    place-items: center;
    color: var(--muted);
    flex: none;
  }
  .twisty span {
    display: grid;
    transition: transform 120ms;
  }
  .twisty span.open {
    transform: rotate(90deg);
  }
  .link {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    align-self: stretch;
    min-width: 0;
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kids {
    font-size: 12px;
  }
  .count {
    color: var(--muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
</style>
