// Client-side entity cache. Every mutation is applied locally first (instant UI),
// then sent to the server; failures roll back. Live changes from other devices
// arrive over SSE and are merged by `rev`.

import { SvelteMap } from 'svelte/reactivity'
import { api, ApiError } from './api/client'
import type { DayEntry } from './api/types/DayEntry'
import type { DayPlan } from './api/types/DayPlan'
import type { FocusSession } from './api/types/FocusSession'
import type { FocusState } from './api/types/FocusState'
import type { FocusTimer } from './api/types/FocusTimer'
import type { Me } from './api/types/Me'
import type { Notification } from './api/types/Notification'
import type { Project } from './api/types/Project'
import type { Series } from './api/types/Series'
import type { SyncResponse } from './api/types/SyncResponse'
import type { Task } from './api/types/Task'
import type { TaskType } from './api/types/TaskType'
import { keyAt, keyBetween } from './order'
import { toast } from './toast.svelte'
import { ulid } from './ulid'

type Kind = 'task' | 'project' | 'day_entry' | 'day_plan' | 'focus_session' | 'series'
type Entity = Task | Project | DayEntry | DayPlan | FocusSession | Series

export type TaskPatch = Partial<
  Pick<
    Task,
    | 'title'
    | 'notes'
    | 'project_id'
    | 'status'
    | 'position'
    | 'due_date'
    | 'estimate_min'
    | 'difficulty'
    | 'importance'
    | 'urgency'
    | 'task_type_id'
    | 'also_project_ids'
  >
>
export type EntryPatch = Partial<Pick<DayEntry, 'date' | 'position' | 'start_time' | 'duration_min'>>

const now = () => new Date().toISOString()
const IDLE_TIMER: FocusTimer = {
  task_id: null,
  phase: 'idle',
  running_since_ms: null,
  elapsed_ms: 0,
  length_min: 0,
  cycle_done: 0,
  rev: 0,
}
const byPosition = (a: { position: string }, b: { position: string }) =>
  a.position < b.position ? -1 : a.position > b.position ? 1 : 0

class Store {
  me = $state<Me | null>(null)
  today = $state('')
  ready = $state(false)
  live = $state(false)
  rev = 0

  taskTypes = new SvelteMap<string, TaskType>()
  projects = new SvelteMap<string, Project>()
  tasks = new SvelteMap<string, Task>()
  entries = new SvelteMap<string, DayEntry>()
  dayPlans = new SvelteMap<string, DayPlan>()
  focusSessions = new SvelteMap<string, FocusSession>()
  series = new SvelteMap<string, Series>()
  focusTimer = $state<FocusTimer>(IDLE_TIMER)
  /** server clock - local clock (ms), from the last response that carried server_now. */
  clockOffset = 0

  /** Called for notifications pushed by the server (e.g. planning reminders). */
  onNotification: ((n: Notification) => void) | null = null

  private pending = new Map<string, number>()
  private es: EventSource | null = null
  private syncChain: Promise<void> = Promise.resolve()
  private deltaQueued: Promise<void> | null = null
  /** Bumped on sign-out so late responses for the previous session are ignored. */
  private generation = 0
  private timer: ReturnType<typeof setInterval> | null = null

  // ---- lifecycle ----------------------------------------------------------

  async start() {
    await this.sync(0)
    this.connect()
    document.addEventListener('visibilitychange', this.onVisible)
    this.timer = setInterval(() => this.refreshToday(), 5 * 60_000)
    this.adoptBrowserTimezone()
  }

  stop() {
    this.generation++
    this.deltaQueued = null
    this.pending.clear()
    this.es?.close()
    this.es = null
    document.removeEventListener('visibilitychange', this.onVisible)
    if (this.timer) clearInterval(this.timer)
    this.me = null
    this.ready = false
    this.live = false
    this.rev = 0
    this.taskTypes.clear()
    this.projects.clear()
    this.tasks.clear()
    this.entries.clear()
    this.dayPlans.clear()
    this.focusSessions.clear()
    this.series.clear()
    this.focusTimer = IDLE_TIMER
  }

  private onVisible = () => {
    if (document.visibilityState === 'visible' && this.me) this.sync().catch(() => {})
  }

  private async refreshToday() {
    try {
      const r = await api.get<{ date: string }>('/today')
      if (r.date !== this.today) await this.sync()
    } catch {
      /* offline */
    }
  }

  /** New accounts start in UTC; adopt the browser's timezone once. */
  private async adoptBrowserTimezone() {
    const tz = Intl.DateTimeFormat().resolvedOptions().timeZone
    const key = 'sl.tzAdopted'
    try {
      if (!this.me || this.me.timezone !== 'UTC' || !tz || tz === 'UTC' || localStorage.getItem(key)) return
      localStorage.setItem(key, '1')
    } catch {
      return
    }
    await this.updateMe({ timezone: tz })
    toast(`Timezone set to ${tz}`)
  }

  /**
   * Fetch changes from the server. `since` omitted = changes since the last known rev
   * (repeated calls while one is waiting are merged); `0` = full reload. Syncs run
   * one at a time, in order.
   */
  sync(since?: number): Promise<void> {
    if (since === undefined && this.deltaQueued) return this.deltaQueued
    const p = this.syncChain
      .catch(() => {})
      .then(() => {
        if (since === undefined) this.deltaQueued = null
        return this.runSync(since ?? this.rev)
      })
    if (since === undefined) this.deltaQueued = p
    this.syncChain = p
    return p
  }

  private async runSync(since: number) {
    const gen = this.generation
    const r = await api.get<SyncResponse>(`/sync?since=${since}`)
    if (gen !== this.generation) return // signed out meanwhile: drop the old user's data
    if (r.full) {
      this.taskTypes.clear()
      this.projects.clear()
      this.tasks.clear()
      this.entries.clear()
      this.dayPlans.clear()
      this.focusSessions.clear()
      this.series.clear()
    }
    this.clockOffset = r.server_now - Date.now()
    if (r.focus_timer.rev >= this.focusTimer.rev) this.focusTimer = r.focus_timer
    this.me = r.me
    this.today = r.today
    for (const t of r.task_types) {
      if (t.deleted_at) this.taskTypes.delete(t.id)
      else this.taskTypes.set(t.id, t)
    }
    for (const p of r.projects) this.applyRemote('project', p)
    for (const t of r.tasks) this.applyRemote('task', t)
    for (const e of r.day_entries) this.applyRemote('day_entry', e)
    for (const p of r.day_plans) this.applyRemote('day_plan', p)
    for (const f of r.focus_sessions) this.applyRemote('focus_session', f)
    for (const x of r.series) this.applyRemote('series', x)
    this.rev = Math.max(this.rev, r.rev)
    this.ready = true
  }

  private connect() {
    this.es?.close()
    const es = new EventSource('/api/v1/events')
    this.es = es
    // `hello` arrives on every (re)connect: catch up on anything we missed.
    es.addEventListener('hello', () => {
      this.live = true
      this.sync().catch(() => {})
    })
    es.addEventListener('change', (ev) => {
      const c = JSON.parse((ev as MessageEvent).data) as { kind: string; data: unknown }
      if (c.kind === 'me') this.me = c.data as Me
      else if (c.kind === 'notification') this.onNotification?.(c.data as Notification)
      else if (c.kind === 'focus_timer') {
        const t = c.data as FocusTimer
        if (t.rev >= this.focusTimer.rev) this.focusTimer = t
      } else if (c.kind === 'focus_session' || c.kind === 'series') this.applyRemote(c.kind, c.data as Entity)
      else if (c.kind === 'task' || c.kind === 'project' || c.kind === 'day_entry' || c.kind === 'day_plan')
        this.applyRemote(c.kind, c.data as Entity)
    })
    es.addEventListener('resync', () => this.sync().catch(() => {}))
    es.onerror = () => {
      this.live = false
      // Closed for good (e.g. session expired): verify auth, retry later.
      if (es.readyState === EventSource.CLOSED) {
        setTimeout(() => {
          if (this.es === es && this.me) {
            this.sync()
              .then(() => this.connect())
              .catch(() => {})
          }
        }, 5000)
      }
    }
  }

  private map(kind: Kind): SvelteMap<string, Entity> {
    const maps = {
      task: this.tasks,
      project: this.projects,
      day_entry: this.entries,
      day_plan: this.dayPlans,
      focus_session: this.focusSessions,
      series: this.series,
    }
    return maps[kind] as SvelteMap<string, Entity>
  }

  private applyRemote(kind: Kind, e: Entity) {
    if (e.rev > this.rev) this.rev = e.rev
    if ((this.pending.get(e.id) ?? 0) > 0) return
    const m = this.map(kind)
    const cur = m.get(e.id)
    if (cur && cur.rev > e.rev) return
    if (e.deleted_at) m.delete(e.id)
    else m.set(e.id, e)
  }

  /**
   * Optimistic mutation: snapshot the touched entities, apply locally, send, then
   * reconcile with the server's response or roll back on failure.
   */
  private async optimistic(
    touched: [Kind, string][],
    apply: () => void,
    send: () => Promise<[Kind, Entity][] | void>,
  ) {
    const snap = touched.map(([k, id]) => [k, id, this.map(k).get(id)] as const)
    for (const [, id] of touched) this.pending.set(id, (this.pending.get(id) ?? 0) + 1)
    apply()
    let results: [Kind, Entity][] | void = undefined
    let failed: unknown = null
    try {
      results = await send()
    } catch (e) {
      failed = e
    }
    for (const [, id] of touched) {
      const n = (this.pending.get(id) ?? 1) - 1
      if (n <= 0) this.pending.delete(id)
      else this.pending.set(id, n)
    }
    if (failed) {
      for (const [k, id, prev] of snap) {
        if (this.pending.has(id)) continue
        if (prev) this.map(k).set(id, prev)
        else this.map(k).delete(id)
      }
      const msg = failed instanceof ApiError ? failed.message : 'Something went wrong'
      toast(msg, 'error')
      this.sync().catch(() => {})
      return
    }
    for (const [k, e] of results ?? []) this.applyRemote(k, e)
  }

  // ---- queries --------------------------------------------------------------

  projectList(includeArchived = false): Project[] {
    return [...this.projects.values()].filter((p) => includeArchived || !p.archived_at).sort(byPosition)
  }

  /** Direct subprojects of `parentId` (`null` = top level), ordered. */
  childProjects(parentId: string | null, includeArchived = false): Project[] {
    return [...this.projects.values()]
      .filter((p) => (p.parent_id ?? null) === parentId && (includeArchived || !p.archived_at))
      .sort(byPosition)
  }

  /** All active projects as a depth-first tree, for nested lists and pickers. */
  projectTree(isCollapsed: (id: string) => boolean = () => false): { project: Project; depth: number }[] {
    const out: { project: Project; depth: number }[] = []
    const walk = (parentId: string | null, depth: number) => {
      for (const p of this.childProjects(parentId)) {
        out.push({ project: p, depth })
        if (!isCollapsed(p.id)) walk(p.id, depth + 1)
      }
    }
    walk(null, 0)
    return out
  }

  /** Ancestors of a project, from the top-level project down to its parent. */
  ancestors(id: string): Project[] {
    const out: Project[] = []
    let cur = this.projects.get(id)?.parent_id
    while (cur && out.length < 100) {
      const p = this.projects.get(cur)
      if (!p) break
      out.unshift(p)
      cur = p.parent_id
    }
    return out
  }

  /** The project and all its subprojects (ids). */
  subtree(id: string): string[] {
    const out = [id]
    for (let i = 0; i < out.length; i++)
      for (const p of this.projects.values()) if (p.parent_id === out[i] && !out.includes(p.id)) out.push(p.id)
    return out
  }

  /** "Paper › Writing" */
  projectPath(id: string): string {
    const p = this.projects.get(id)
    return p ? [...this.ancestors(id), p].map((x) => x.name).join(' › ') : ''
  }

  /** Open tasks in the project including all its subprojects. */
  openCountDeep(id: string): number {
    const ids = new Set(this.subtree(id))
    let n = 0
    for (const t of this.tasks.values())
      if (t.status === 'open' && ((t.project_id && ids.has(t.project_id)) || t.also_project_ids.some((p) => ids.has(p)))) n++
    return n
  }

  /**
   * Routine occurrences for a later day (they exist from the day before, or when a
   * future day is viewed) stay out of lists until their day.
   */
  isUpcoming(t: Task, day = this.today): boolean {
    return !!t.occurrence_date && t.occurrence_date > day
  }

  /** Tasks in a project (`null` = inbox), ordered. */
  tasksIn(projectId: string | null, status: 'open' | 'closed' = 'open'): Task[] {
    return [...this.tasks.values()]
      .filter(
        (t) =>
          (t.project_id === projectId || (projectId !== null && t.also_project_ids.includes(projectId))) &&
          !this.isUpcoming(t) &&
          (status === 'open' ? t.status === 'open' : t.status !== 'open'),
      )
      .sort(status === 'open' ? byPosition : (a, b) => ((a.completed_at ?? a.updated_at) < (b.completed_at ?? b.updated_at) ? 1 : -1))
  }

  entryForTask(taskId: string): DayEntry | undefined {
    for (const e of this.entries.values()) if (e.task_id === taskId) return e
    return undefined
  }

  dayEntries(date: string): DayEntry[] {
    return [...this.entries.values()].filter((e) => e.date === date).sort(byPosition)
  }

  /** Open tasks that could be pulled into `date`. */
  readyStack(date: string): Task[] {
    const plannedHere = new Set(this.dayEntries(date).map((e) => e.task_id))
    return [...this.tasks.values()]
      .filter((t) => t.status === 'open' && !plannedHere.has(t.id) && !this.isUpcoming(t, date))
      .sort(byPosition)
  }

  taskType(id: string) {
    return this.taskTypes.get(id)
  }

  // ---- tasks ----------------------------------------------------------------

  createTask(input: { title: string; project_id?: string | null; day?: string } & TaskPatch): string {
    const id = ulid()
    const projectId = input.project_id ?? null
    const siblings = this.tasksIn(projectId)
    const ts = now()
    const task: Task = {
      id,
      owner_user_id: this.me?.id ?? null,
      owner_group_id: null,
      assignee_user_id: null,
      project_id: projectId,
      title: input.title,
      notes: input.notes ?? '',
      status: 'open',
      position: keyBetween(siblings.at(-1)?.position, null),
      due_date: input.due_date ?? null,
      estimate_min: input.estimate_min ?? null,
      difficulty: input.difficulty ?? null,
      importance: input.importance ?? null,
      urgency: input.urgency ?? null,
      actual_min: 0,
      task_type_id: input.task_type_id ?? 'tt_carry_on',
      carry_count: 0,
      started_at: null,
      completed_at: null,
      completed_by: null,
      ext_source: null,
      ext_id: null,
      ext_url: null,
      also_project_ids: [],
      series_id: null,
      occurrence_key: null,
      occurrence_date: null,
      window_end: null,
      created_at: ts,
      updated_at: ts,
      deleted_at: null,
      rev: 0,
    }
    const touched: [Kind, string][] = [['task', id]]
    let entry: DayEntry | null = null
    if (input.day) {
      const day = this.dayEntries(input.day)
      entry = {
        id: ulid(),
        user_id: this.me?.id ?? '',
        date: input.day,
        task_id: id,
        position: keyBetween(day.at(-1)?.position, null),
        start_time: null,
        duration_min: null,
        created_at: ts,
        updated_at: ts,
        deleted_at: null,
        rev: 0,
      }
      touched.push(['day_entry', entry.id])
    }
    this.optimistic(
      touched,
      () => {
        this.tasks.set(id, task)
        if (entry) this.entries.set(entry.id, entry)
      },
      async () => {
        const { day, ...rest } = input
        const t = await api.post<Task>('/tasks', {
          ...rest,
          id,
          project_id: projectId,
          position: task.position,
          day: day ?? null,
          day_entry_id: entry?.id ?? null,
        })
        return [['task', t]]
      },
    )
    return id
  }

  updateTask(id: string, patch: TaskPatch) {
    const cur = this.tasks.get(id)
    if (!cur) return
    const local: Task = { ...cur, ...patch, updated_at: now() }
    if (patch.status && patch.status !== cur.status) {
      local.completed_at = patch.status === 'done' ? now() : null
      local.completed_by = patch.status === 'done' ? (this.me?.id ?? null) : null
      if (patch.status === 'open') local.started_at = null
    }
    this.optimistic(
      [['task', id]],
      () => this.tasks.set(id, local),
      async () => [['task', await api.patch<Task>(`/tasks/${id}`, patch)]],
    )
  }

  toggleDone(id: string) {
    const t = this.tasks.get(id)
    if (!t) return
    const done = t.status !== 'done'
    this.updateTask(id, { status: done ? 'done' : 'open' })
    if (done) toast(`Completed “${t.title}”`, 'info', { label: 'Undo', run: () => this.updateTask(id, { status: 'open' }) })
  }

  deleteTask(id: string) {
    const t = this.tasks.get(id)
    if (!t) return
    const e = this.entryForTask(id)
    const touched: [Kind, string][] = [['task', id]]
    if (e) touched.push(['day_entry', e.id])
    this.optimistic(
      touched,
      () => {
        this.tasks.delete(id)
        if (e) this.entries.delete(e.id)
      },
      () => api.del(`/tasks/${id}`),
    )
  }

  /** Move a task to `index` within `list` (the currently displayed order). */
  reorderTask(list: Task[], id: string, index: number) {
    const others = list.filter((t) => t.id !== id).map((t) => t.position)
    this.updateTask(id, { position: keyAt(others, index) })
  }

  // ---- projects -------------------------------------------------------------

  createProject(name: string, parentId: string | null = null): string {
    const id = ulid()
    const ts = now()
    const parent = parentId ? this.projects.get(parentId) : null
    const p: Project = {
      id,
      owner_user_id: this.me?.id ?? null,
      owner_group_id: null,
      parent_id: parentId,
      name,
      color: parent?.color ?? null,
      position: keyBetween(this.childProjects(parentId, true).at(-1)?.position, null),
      archived_at: null,
      created_at: ts,
      updated_at: ts,
      deleted_at: null,
      rev: 0,
    }
    this.optimistic(
      [['project', id]],
      () => this.projects.set(id, p),
      async () => [
        ['project', await api.post<Project>('/projects', { id, name, parent_id: parentId, color: p.color, position: p.position })],
      ],
    )
    return id
  }

  updateProject(
    id: string,
    patch: { name?: string; color?: string | null; position?: string; archived?: boolean; parent_id?: string | null },
  ) {
    const cur = this.projects.get(id)
    if (!cur) return
    const { archived, ...rest } = patch
    const ts = now()
    // Archiving applies to the whole subtree (the server does the same).
    const touched = archived === undefined ? [id] : this.subtree(id)
    const updated = touched.map((pid) => {
      const p = pid === id ? { ...cur, ...rest } : { ...this.projects.get(pid)! }
      if (archived !== undefined) p.archived_at = archived ? (p.archived_at ?? ts) : null
      return { ...p, updated_at: ts }
    })
    this.optimistic(
      touched.map((pid) => ['project', pid] as [Kind, string]),
      () => updated.forEach((p) => this.projects.set(p.id, p)),
      async () => [['project', await api.patch<Project>(`/projects/${id}`, patch)]],
    )
  }

  reorderProject(list: Project[], id: string, index: number) {
    const others = list.filter((p) => p.id !== id).map((p) => p.position)
    this.updateProject(id, { position: keyAt(others, index) })
  }

  /** Can `id` be moved under `parentId`? Not into itself or its own subprojects. */
  canMoveProject(id: string, parentId: string | null): boolean {
    return parentId === null || !this.subtree(id).includes(parentId)
  }

  /** Nest a project under another (`null` = top level), optionally at `index` among its new siblings. */
  moveProject(id: string, parentId: string | null, index?: number) {
    if (!this.canMoveProject(id, parentId)) {
      toast("A project can't go inside itself or one of its subprojects", 'error')
      return
    }
    const siblings = this.childProjects(parentId, true).filter((p) => p.id !== id)
    const position = index === undefined ? keyBetween(siblings.at(-1)?.position, null) : keyAt(siblings.map((p) => p.position), index)
    this.updateProject(id, { parent_id: parentId, position })
  }

  /** Deletes the project and all its subprojects, with their tasks. */
  deleteProject(id: string) {
    const ids = new Set(this.subtree(id))
    const tasks = [...this.tasks.values()].filter((t) => t.project_id && ids.has(t.project_id))
    const entries = tasks.map((t) => this.entryForTask(t.id)).filter((e): e is DayEntry => !!e)
    const touched: [Kind, string][] = [
      ...[...ids].map((pid) => ['project', pid] as [Kind, string]),
      ...tasks.map((t) => ['task', t.id] as [Kind, string]),
      ...entries.map((e) => ['day_entry', e.id] as [Kind, string]),
    ]
    this.optimistic(
      touched,
      () => {
        // Tasks only also listed in the deleted projects just lose the link.
        for (const t of this.tasks.values())
          if (t.also_project_ids.some((p) => ids.has(p)) && !tasks.includes(t))
            this.tasks.set(t.id, { ...t, also_project_ids: t.also_project_ids.filter((p) => !ids.has(p)) })
        ids.forEach((pid) => this.projects.delete(pid))
        tasks.forEach((t) => this.tasks.delete(t.id))
        entries.forEach((e) => this.entries.delete(e.id))
      },
      () => api.del(`/projects/${id}`),
    )
  }

  // ---- day plan -------------------------------------------------------------

  /**
   * Plan a task into a day (moves it if it is planned elsewhere). Options left out
   * keep their current value on the same day, or default when the day changes:
   * appended to the end, no time slot.
   */
  plan(taskId: string, date: string, opts: { position?: string; startTime?: string | null } = {}) {
    const existing = this.entryForTask(taskId)
    const sameDay = existing?.date === date
    const last = this.dayEntries(date)
      .filter((e) => e.task_id !== taskId)
      .at(-1)
    const position = opts.position ?? (sameDay ? existing!.position : keyBetween(last?.position, null))
    const start_time = opts.startTime !== undefined ? opts.startTime : sameDay ? existing!.start_time : null
    const ts = now()
    const entry: DayEntry = existing
      ? { ...existing, date, position, start_time, updated_at: ts }
      : {
          id: ulid(),
          user_id: this.me?.id ?? '',
          date,
          task_id: taskId,
          position,
          start_time,
          duration_min: null,
          created_at: ts,
          updated_at: ts,
          deleted_at: null,
          rev: 0,
        }
    this.optimistic(
      [['day_entry', entry.id]],
      () => this.entries.set(entry.id, entry),
      async () => [
        [
          'day_entry',
          await api.post<DayEntry>(`/days/${date}/entries`, {
            id: entry.id,
            task_id: taskId,
            position,
            start_time,
            duration_min: entry.duration_min,
          }),
        ],
      ],
    )
  }

  /** Move a task into a project (null = inbox), optionally at a given position. */
  moveToProject(id: string, projectId: string | null, position?: string) {
    const t = this.tasks.get(id)
    if (!t) return
    if (t.project_id === projectId && position === undefined) return
    const last = this.tasksIn(projectId).filter((x) => x.id !== id).at(-1)
    this.updateTask(id, {
      project_id: projectId,
      position: position ?? keyBetween(last?.position, null),
      // Becoming the main project replaces an "also in" link to it.
      also_project_ids: t.also_project_ids.filter((p) => p !== projectId),
    })
  }

  /** Also list a task in other projects (besides its main one). */
  setAlsoProjects(id: string, projectIds: string[]) {
    const t = this.tasks.get(id)
    if (!t) return
    this.updateTask(id, { also_project_ids: [...new Set(projectIds)].filter((p) => p !== t.project_id) })
  }

  updateEntry(id: string, patch: EntryPatch) {
    const cur = this.entries.get(id)
    if (!cur) return
    this.optimistic(
      [['day_entry', id]],
      () => this.entries.set(id, { ...cur, ...patch, updated_at: now() }),
      async () => [['day_entry', await api.patch<DayEntry>(`/day-entries/${id}`, patch)]],
    )
  }

  reorderEntry(list: DayEntry[], id: string, index: number) {
    const others = list.filter((e) => e.id !== id).map((e) => e.position)
    this.updateEntry(id, { position: keyAt(others, index) })
  }

  unplan(entryId: string) {
    const cur = this.entries.get(entryId)
    if (!cur) return
    this.optimistic(
      [['day_entry', entryId]],
      () => this.entries.delete(entryId),
      () => api.del(`/day-entries/${entryId}`),
    )
  }

  // ---- planning ritual -------------------------------------------------------

  /** The planning record of a day (undefined = unplanned). */
  planFor(date: string): DayPlan | undefined {
    for (const p of this.dayPlans.values()) if (p.date === date) return p
    return undefined
  }

  isPlanned(date: string): boolean {
    return this.planFor(date)?.status === 'planned'
  }

  /** Save wizard progress (`draft`) or confirm the plan (`planned`). Planned stays planned. */
  setPlan(date: string, status: 'draft' | 'planned', step: number) {
    const cur = this.planFor(date)
    const ts = now()
    const next: DayPlan = cur
      ? {
          ...cur,
          status: status === 'planned' || cur.status !== 'planned' ? status : 'planned',
          step,
          planned_at: cur.planned_at ?? (status === 'planned' ? ts : null),
          updated_at: ts,
        }
      : {
          id: ulid(),
          user_id: this.me?.id ?? '',
          date,
          status,
          step,
          planned_at: status === 'planned' ? ts : null,
          created_at: ts,
          updated_at: ts,
          deleted_at: null,
          rev: 0,
        }
    this.optimistic(
      [['day_plan', next.id]],
      () => this.dayPlans.set(next.id, next),
      async () => [['day_plan', await api.put<DayPlan>(`/days/${date}/plan`, { id: next.id, status, step })]],
    )
  }

  /** Mark a day as unplanned again. */
  clearPlan(date: string) {
    const cur = this.planFor(date)
    if (!cur) return
    this.optimistic(
      [['day_plan', cur.id]],
      () => this.dayPlans.delete(cur.id),
      () => api.del(`/days/${date}/plan`),
    )
  }

  // ---- focus timer -----------------------------------------------------------

  /** Current time on the server's clock (ms). */
  serverNow(): number {
    return Date.now() + this.clockOffset
  }

  /** Remaining ms of the current interval. */
  focusRemaining(at = this.serverNow()): number {
    const t = this.focusTimer
    const elapsed = t.elapsed_ms + (t.running_since_ms !== null ? Math.max(0, at - t.running_since_ms) : 0)
    return Math.max(0, t.length_min * 60_000 - elapsed)
  }

  /**
   * Control the timer. Start/pause/resume/stop show immediately; the server's answer
   * (which also completes anything due and logs sessions) then replaces the local state.
   */
  async focus(action: 'start' | 'pause' | 'resume' | 'skip' | 'stop' | 'sync', taskId?: string | null) {
    const prev = this.focusTimer
    const at = this.serverNow()
    const t = { ...prev }
    if (action === 'start')
      Object.assign(t, {
        phase: 'work',
        task_id: taskId ?? null,
        running_since_ms: at,
        elapsed_ms: 0,
        length_min: this.me?.focus_work_min ?? 25,
        cycle_done: prev.phase === 'long_break' ? 0 : prev.cycle_done,
      })
    else if (action === 'pause' && t.running_since_ms !== null)
      Object.assign(t, { elapsed_ms: t.elapsed_ms + (at - t.running_since_ms), running_since_ms: null })
    else if (action === 'resume' && t.running_since_ms === null && t.phase !== 'idle') t.running_since_ms = at
    else if (action === 'stop') Object.assign(t, IDLE_TIMER, { rev: prev.rev })
    this.focusTimer = t
    if (action === 'start' && taskId) {
      const task = this.tasks.get(taskId)
      if (task && task.status === 'open' && !task.started_at) this.tasks.set(taskId, { ...task, started_at: now() })
    }
    try {
      const r = await api.post<FocusState>('/focus', { action, task_id: taskId ?? null })
      this.clockOffset = r.server_now - Date.now()
      if (r.timer.rev >= this.focusTimer.rev) this.focusTimer = r.timer
    } catch (e) {
      this.focusTimer = prev
      toast(e instanceof ApiError ? e.message : 'Could not reach the server', 'error')
    }
  }

  /** Focus sessions, newest first, optionally for one task and/or since a time. */
  sessions(opts: { taskId?: string; since?: string; kind?: 'work' | 'break' } = {}): FocusSession[] {
    return [...this.focusSessions.values()]
      .filter(
        (s) =>
          (!opts.taskId || s.task_id === opts.taskId) &&
          (!opts.since || s.started_at >= opts.since) &&
          (!opts.kind || s.kind === opts.kind),
      )
      .sort((a, b) => (a.started_at < b.started_at ? 1 : -1))
  }

  // ---- in progress ----------------------------------------------------------

  setInProgress(id: string, inProgress: boolean) {
    const cur = this.tasks.get(id)
    if (!cur) return
    this.optimistic(
      [['task', id]],
      () => this.tasks.set(id, { ...cur, started_at: inProgress ? (cur.started_at ?? now()) : null, updated_at: now() }),
      async () => [['task', await api.patch<Task>(`/tasks/${id}`, { in_progress: inProgress })]],
    )
  }

  // ---- preferences ----------------------------------------------------------

  pref<T>(key: string, fallback: T): T {
    const v = this.me?.prefs?.[key]
    return v === undefined || v === null ? fallback : (v as T)
  }

  /** Save a UI preference for this user (shared across devices); `null` removes it. */
  async setPref(key: string, value: unknown) {
    if (!this.me) return
    const prev = this.me.prefs
    const next = { ...prev }
    if (value === null) delete next[key]
    else next[key] = value
    this.me = { ...this.me, prefs: next }
    try {
      const me = await api.patch<Me>('/me/prefs', { [key]: value })
      if (this.me) this.me = { ...this.me, prefs: me.prefs }
    } catch {
      if (this.me) this.me = { ...this.me, prefs: prev }
    }
  }

  // ---- routines ------------------------------------------------------------

  /** Routines that are still running (not ended before today). */
  activeSeries(): Series[] {
    return [...this.series.values()]
      .filter((s) => !s.until || s.until >= this.today)
      .sort((a, b) => a.title.localeCompare(b.title))
  }

  /** Done/total of a flexible routine's occurrences in the window that contains `day`. */
  windowProgress(seriesId: string, day: string): { done: number; total: number } {
    let done = 0
    let total = 0
    for (const t of this.tasks.values())
      if (t.series_id === seriesId && t.window_end && t.occurrence_date! <= day && day <= t.window_end) {
        total++
        if (t.status === 'done') done++
      }
    return { done, total }
  }

  async createSeries(input: Record<string, unknown>): Promise<Series | null> {
    try {
      const s = await api.post<Series>('/series', { id: ulid(), ...input })
      this.applyRemote('series', s)
      return s
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Could not save the routine', 'error')
      return null
    }
  }

  /** Change a routine from `from` on (default today). May return a new routine (split). */
  async updateSeries(id: string, patch: Record<string, unknown>): Promise<Series | null> {
    try {
      const s = await api.patch<Series>(`/series/${id}`, patch)
      this.applyRemote('series', s)
      await this.sync()
      return s
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Could not save the routine', 'error')
      return null
    }
  }

  /** End a routine; its history stays. */
  endSeries(id: string) {
    const cur = this.series.get(id)
    if (!cur) return
    const open = [...this.tasks.values()].filter(
      (t) => t.series_id === id && t.status === 'open' && (t.occurrence_date ?? '') >= this.today,
    )
    this.optimistic(
      [['series', id], ...open.map((t) => ['task', t.id] as [Kind, string])],
      () => {
        this.series.delete(id)
        open.forEach((t) => this.tasks.delete(t.id))
      },
      () => api.del(`/series/${id}`),
    )
  }

  // ---- account --------------------------------------------------------------

  async updateMe(
    patch: Partial<
      Pick<
        Me,
        | 'display_name'
        | 'timezone'
        | 'day_end'
        | 'locale'
        | 'week_start'
        | 'plan_mode'
        | 'plan_time_evening'
        | 'plan_time_morning'
        | 'day_window_start'
        | 'day_window_end'
        | 'focus_work_min'
        | 'focus_short_break_min'
        | 'focus_long_break_min'
        | 'focus_long_every'
      >
    >,
  ) {
    try {
      this.me = await api.patch<Me>('/me', patch)
      await this.sync()
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Could not save', 'error')
    }
  }
}

export const store = new Store()
