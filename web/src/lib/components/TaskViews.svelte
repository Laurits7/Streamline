<script lang="ts">
  // The view switcher plus the chosen view for one scope (all tasks, the inbox, or a
  // project with its subprojects). The list view is supplied by the page.
  import type { Snippet } from 'svelte'
  import type { Task } from '../api/types/Task'
  import { store } from '../store.svelte'
  import { DEFAULT_VIEW, type BoardGroup, type ViewPrefs } from '../views'
  import Board from './Board.svelte'
  import Matrix from './Matrix.svelte'
  import ViewSwitcher from './ViewSwitcher.svelte'

  type Scope = { kind: 'all' } | { kind: 'inbox' } | { kind: 'project'; id: string }
  let { scope, list }: { scope: Scope; list: Snippet } = $props()

  const prefKey = $derived(scope.kind === 'project' ? `view:project:${scope.id}` : `view:${scope.kind}`)
  const prefs = $derived({ ...DEFAULT_VIEW, ...store.pref<Partial<ViewPrefs>>(prefKey, {}) })

  // Open tasks of the scope, plus ones completed in the last two weeks (for the board).
  const tasks = $derived.by((): Task[] => {
    const since = new Date(Date.now() - 14 * 864e5).toISOString()
    const ids = scope.kind === 'project' ? new Set(store.subtree(scope.id)) : null
    return [...store.tasks.values()].filter((t) => {
      const inScope =
        scope.kind === 'all' ||
        (scope.kind === 'inbox'
          ? t.project_id === null
          : (t.project_id !== null && ids!.has(t.project_id)) || t.also_project_ids.some((p) => ids!.has(p)))
      return inScope && !store.isUpcoming(t) && (t.status === 'open' || (t.status === 'done' && (t.completed_at ?? '') >= since))
    })
  })

  const projectColumns = $derived.by(() => {
    if (scope.kind === 'project') {
      const p = store.projects.get(scope.id)
      return [
        { id: scope.id, title: p?.name ?? '', color: p?.color ?? null, own: true },
        ...store.childProjects(scope.id).map((c) => ({ id: c.id, title: c.name, color: c.color })),
      ]
    }
    return [
      { id: null, title: 'Inbox', color: null },
      ...store.childProjects(null).map((c) => ({ id: c.id, title: c.name, color: c.color })),
    ]
  })

  const groups = $derived(
    [
      { id: 'status' as BoardGroup, label: 'Status' },
      ...(scope.kind === 'inbox' ? [] : [{ id: 'project' as BoardGroup, label: scope.kind === 'all' ? 'Project' : 'Subproject' }]),
      { id: 'difficulty' as BoardGroup, label: 'Difficulty' },
      { id: 'type' as BoardGroup, label: 'Task type' },
    ],
  )
  const group = $derived(groups.some((g) => g.id === prefs.group) ? prefs.group : 'status')
</script>

<ViewSwitcher {prefKey} {groups} />
{#if prefs.view === 'board'}
  <Board {tasks} {group} {projectColumns} />
{:else if prefs.view === 'matrix'}
  <Matrix {tasks} />
{:else}
  {@render list()}
{/if}
