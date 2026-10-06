<script lang="ts">
  import type { DayEntry } from '../api/types/DayEntry'
  import type { Task } from '../api/types/Task'
  import { fmtMinutes, shortDate } from '../dates'
  import { draggable } from '../dnd.svelte'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'
  import Check from './Check.svelte'
  import Icon from './Icon.svelte'

  let {
    task,
    entry = null,
    showProject = false,
    handle = false,
    planButton = false,
    planDate = null,
  }: {
    task: Task
    entry?: DayEntry | null
    showProject?: boolean
    /** Show a grip: keyboard reorder (arrow keys) and immediate touch drag. */
    handle?: boolean
    planButton?: boolean
    planDate?: string | null
  } = $props()

  const project = $derived(task.project_id ? store.projects.get(task.project_id) : null)
  const type = $derived(store.taskType(task.task_type_id))
  const plannedEntry = $derived(planButton ? store.entryForTask(task.id) : null)
  const target = $derived(planDate ?? store.today)
  const plannedHere = $derived(plannedEntry?.date === target)
  const overdue = $derived(task.status === 'open' && !!task.due_date && task.due_date < store.today)
  const closed = $derived(task.status !== 'open')
  const statusLabel: Record<string, string> = { missed: 'Missed', skipped: 'Skipped', wont_do: "Won't do" }
</script>

<div
  class="row"
  class:closed
  data-id={entry?.id ?? task.id}
  use:draggable={{
    disabled: task.status !== 'open',
    item: () => ({
      kind: 'task',
      taskId: task.id,
      entryId: entry?.id,
      durationMin: entry?.duration_min ?? task.estimate_min ?? 30,
    }),
  }}>
  {#if handle}
    <button class="handle" data-handle aria-label="Reorder {task.title} (drag, or use arrow keys)">
      <Icon name="grip" size={16} />
    </button>
  {/if}
  {#if task.status === 'open' || task.status === 'done'}
    <span data-nodrag class="check-wrap"><Check
      done={task.status === 'done'}
      onclick={() => store.toggleDone(task.id)}
      label={task.status === 'done' ? `Mark ${task.title} as not done` : `Complete ${task.title}`} /></span>
  {:else}
    <span class="status-dot" title={statusLabel[task.status]}><Icon name="x" size={12} /></span>
  {/if}
  <button class="main" onclick={() => (ui.editing = task.id)}>
    <span class="title">
      {#if entry?.start_time}<span class="time">{entry.start_time}</span>{/if}
      {task.title}
    </span>
    <span class="meta">
      {#if statusLabel[task.status]}<span class="tag danger">{statusLabel[task.status]}</span>{/if}
      {#if showProject && project}<span class="project" title={store.projectPath(project.id)}><i style:background={project.color ?? 'var(--faint)'}></i>{project.name}</span>{/if}
      {#if task.due_date}<span class:overdue><Icon name="calendar" size={12} />{shortDate(task.due_date, store.today)}</span>{/if}
      {#if task.estimate_min}<span><Icon name="clock" size={12} />{fmtMinutes(task.estimate_min)}</span>{/if}
      {#if task.difficulty}<span class="diff" title="Difficulty">{'●'.repeat(task.difficulty)}{'○'.repeat(3 - task.difficulty)}</span>{/if}
      {#if (task.importance ?? 0) >= 2}<span class="tag accent">Important</span>{/if}
      {#if (task.urgency ?? 0) >= 2}<span class="tag warn">Urgent</span>{/if}
      {#if task.carry_count > 0 && type?.shows_carry_count && task.status === 'open'}<span title="Carried over {task.carry_count} day(s)"><Icon name="repeat" size={12} />{task.carry_count}d</span>{/if}
      {#if type && type.key !== 'carry_on' && !(task.window_end && type.key === 'window')}<span class="tag">{type.name}</span>{/if}
      {#if task.series_id}
        {@const prog = task.window_end ? store.windowProgress(task.series_id, store.today) : null}
        <span title="Routine"><Icon name="repeat" size={12} />{prog && prog.total > 1 ? ` ${prog.done}/${prog.total} this ${store.series.get(task.series_id)?.window ?? 'week'}` : ''}</span>
      {/if}
      {#if task.notes.trim()}<span title="Has notes">¶</span>{/if}
    </span>
  </button>
  {#if planButton && task.status === 'open'}
    <button
      data-nodrag
      class="icon-btn plan"
      class:on={plannedHere}
      aria-label={plannedHere ? `Remove ${task.title} from ${shortDate(target, store.today)}` : `Plan ${task.title} for ${shortDate(target, store.today)}`}
      title={plannedHere ? 'Planned — tap to remove' : plannedEntry ? `Planned for ${shortDate(plannedEntry.date, store.today)}` : 'Plan for today'}
      onclick={() => (plannedHere && plannedEntry ? store.unplan(plannedEntry.id) : store.plan(task.id, target))}>
      <Icon name="sun" />
      {#if plannedEntry && !plannedHere}<span class="badge">{shortDate(plannedEntry.date, store.today).slice(0, 3)}</span>{/if}
    </button>
  {/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px 6px 12px;
    min-height: 52px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .check-wrap {
    display: contents;
  }
  .row:last-child {
    border-bottom: 0;
  }
  .handle {
    display: grid;
    place-items: center;
    width: 24px;
    height: 36px;
    margin-left: -6px;
    color: var(--faint);
    cursor: grab;
    touch-action: none;
    border-radius: 6px;
  }
  .handle:active {
    cursor: grabbing;
  }
  .main {
    flex: 1;
    min-width: 0;
    text-align: left;
    padding: 6px 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .title {
    overflow-wrap: anywhere;
  }
  .closed .title {
    color: var(--muted);
    text-decoration: line-through;
    text-decoration-color: var(--faint);
  }
  .time {
    font-variant-numeric: tabular-nums;
    font-weight: 650;
    color: var(--accent);
    margin-right: 6px;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    font-size: 12px;
    color: var(--muted);
  }
  .meta:empty {
    display: none;
  }
  .meta span {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .project i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
  }
  .overdue {
    color: var(--danger);
    font-weight: 600;
  }
  .diff {
    letter-spacing: -1px;
    font-size: 9px;
  }
  .tag {
    padding: 0 6px;
    border-radius: 4px;
    background: var(--surface-2);
    font-weight: 600;
    font-size: 11px;
  }
  .tag.accent {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .tag.warn {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .tag.danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .status-dot {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--danger-soft);
    color: var(--danger);
  }
  .plan {
    position: relative;
  }
  .plan.on {
    color: var(--warn);
    background: var(--warn-soft);
  }
  .badge {
    position: absolute;
    bottom: 0;
    right: -2px;
    font-size: 9px;
    font-weight: 700;
    background: var(--surface-3);
    color: var(--muted);
    border-radius: 4px;
    padding: 0 3px;
  }
</style>
