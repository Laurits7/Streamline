// Client-side entity cache. Every mutation is applied locally first (instant UI),
// then sent to the server; failures roll back. Live changes from other devices
// arrive over SSE and are merged by `rev`.

import { SvelteMap } from 'svelte/reactivity'
import { api, ApiError } from './api/client'
import type { Calendar } from './api/types/Calendar'
import type { CalendarAccountView } from './api/types/CalendarAccountView'
import type { CalendarEvent } from './api/types/CalendarEvent'
import type { EventProject } from './api/types/EventProject'
import type { DayEntry } from './api/types/DayEntry'
import type { Goal } from './api/types/Goal'
import type { GoalProgress } from './api/types/GoalProgress'
import type { DayRecord } from './api/types/DayRecord'
import type { MetricDefinition } from './api/types/MetricDefinition'
import type { MetricEntry } from './api/types/MetricEntry'
import type { DayTemplate } from './api/types/DayTemplate'
import type { SuggestedPlan } from './api/types/SuggestedPlan'
import type { TemplateBlock } from './api/types/TemplateBlock'
import type { TimeBlock } from './api/types/TimeBlock'
import type { NamedayCalendar } from './api/types/NamedayCalendar'
import type { NameMatch } from './api/types/NameMatch'
import type { OccasionStep } from './api/types/OccasionStep'
import type { OccasionTemplate } from './api/types/OccasionTemplate'
import type { Person } from './api/types/Person'
import type { DayPlan } from './api/types/DayPlan'
import type { FocusSession } from './api/types/FocusSession'
import type { FocusState } from './api/types/FocusState'
import type { FocusTimer } from './api/types/FocusTimer'
import type { Me } from './api/types/Me'
import type { Notification } from './api/types/Notification'
import type { Place } from './api/types/Place'
import type { Project } from './api/types/Project'
import type { Series } from './api/types/Series'
import type { Group } from './api/types/Group'
import type { UserSummary } from './api/types/UserSummary'
import type { WorkflowTemplate } from './api/types/WorkflowTemplate'
import type { TaskTemplate } from './api/types/TaskTemplate'
import type { SyncResponse } from './api/types/SyncResponse'
import type { Task } from './api/types/Task'
import type { TaskType } from './api/types/TaskType'
import { busyIntervals, eventsOn, zoned } from './calendar'
import { daily } from './tracking'
import { reviewDue, type Cadence } from './goals'
import { findConflicts, nextFreeSlot, type Conflict, type Item } from './conflicts'
import { isBlocked } from './deps'
import { keyAt, keyBetween } from './order'
import { toast } from './toast.svelte'
import { ulid } from './ulid'
import { pickSub, pickTop, type Shape } from './marks'
import { checkBackDue, isWaiting } from './waiting'

type Kind =
  | 'task'
  | 'project'
  | 'day_entry'
  | 'day_plan'
  | 'focus_session'
  | 'series'
  | 'place'
  | 'workflow'
  | 'task_template'
  | 'group'
  | 'calendar'
  | 'event'
  | 'event_project'
  | 'person'
  | 'day_template'
  | 'time_block'
  | 'day_record'
  | 'metric'
  | 'metric_entry'
  | 'goal'
type Entity =
  | Task
  | Project
  | DayEntry
  | DayPlan
  | FocusSession
  | Series
  | Place
  | WorkflowTemplate
  | TaskTemplate
  | Group
  | Calendar
  | CalendarEvent
  | EventProject
  | Person
  | DayTemplate
  | TimeBlock
  | DayRecord
  | MetricDefinition
  | MetricEntry
  | Goal

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
    | 'place_id'
    | 'event_id'
    | 'wait_min'
    | 'checklist'
  >
>
export type EntryPatch = Partial<Pick<DayEntry, 'date' | 'position' | 'start_time' | 'duration_min'>>

const now = () => new Date().toISOString()
const NOT_WAITING = { waiting_since: null, check_back_at: null, waiting_note: '', waiting_by: null }
function readLocal(key: string, fallback: string): string {
  try {
    return localStorage.getItem(key) ?? fallback
  } catch {
    return fallback
  }
}
function writeLocal(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* storage unavailable */
  }
}
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

/** Lowercase without accents, like the server's name search (`Tõnu` ~ "tonu"). */
export const fold = (s: string) =>
  s
    .trim()
    .toLowerCase()
    .normalize('NFD')
    .replace(/\p{M}/gu, '')

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
  places = new SvelteMap<string, Place>()
  workflows = new SvelteMap<string, WorkflowTemplate>()
  taskTemplates = new SvelteMap<string, TaskTemplate>()
  groups = new SvelteMap<string, Group>()
  calendars = new SvelteMap<string, Calendar>()
  events = new SvelteMap<string, CalendarEvent>()
  /** Calendar events (by calendar + UID) assigned to projects. */
  eventProjects = new SvelteMap<string, EventProject>()
  calendarAccount = $state<CalendarAccountView | null>(null)
  people = new SvelteMap<string, Person>()
  dayTemplates = new SvelteMap<string, DayTemplate>()
  dayRecords = new SvelteMap<string, DayRecord>()
  metrics = new SvelteMap<string, MetricDefinition>()
  metricEntries = new SvelteMap<string, MetricEntry>()
  goals = new SvelteMap<string, Goal>()
  goalProgress = new SvelteMap<string, GoalProgress>()
  timeBlocks = new SvelteMap<string, TimeBlock>()
  occasionTemplates = new SvelteMap<string, OccasionTemplate>()
  /** The shared nameday calendar, loaded on first use: `MM-DD` → names. */
  namedays = $state<{ label: string | null; byDate: Map<string, string[]> } | null>(null)
  private namedaysLoading: Promise<void> | null = null
  /** Where this device is ('' = anywhere/not set). Per device, like the GPS setting (D-45). */
  currentPlace = $state(readLocal('sl.place', ''))
  /** Use GPS to set the current place (per device; needs HTTPS and permission). */
  useGps = $state(readLocal('sl.gps', '') === '1')
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
    this.places.clear()
    this.workflows.clear()
    this.taskTemplates.clear()
    this.groups.clear()
    this.calendars.clear()
    this.events.clear()
    this.eventProjects.clear()
    this.calendarAccount = null
    this.people.clear()
    this.dayTemplates.clear()
    this.timeBlocks.clear()
    this.dayRecords.clear()
    this.metrics.clear()
    this.metricEntries.clear()
    this.goals.clear()
    this.goalProgress.clear()
    this.occasionTemplates.clear()
    this.namedays = null
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
      this.places.clear()
      this.workflows.clear()
      this.taskTemplates.clear()
      this.calendars.clear()
      this.events.clear()
      this.people.clear()
      this.dayTemplates.clear()
      this.timeBlocks.clear()
      this.dayRecords.clear()
      this.metrics.clear()
      this.metricEntries.clear()
      this.goals.clear()
    }
    this.occasionTemplates.clear()
    for (const x of r.occasion_templates) this.occasionTemplates.set(x.kind, x)
    this.calendarAccount = r.calendar_account
    // Groups always come complete.
    this.groups.clear()
    for (const g of r.groups) this.groups.set(g.id, g)
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
    for (const x of r.places) this.applyRemote('place', x)
    for (const x of r.workflows) this.applyRemote('workflow', x)
    for (const x of r.task_templates) this.applyRemote('task_template', x)
    for (const x of r.calendars) this.applyRemote('calendar', x)
    for (const x of r.events) this.applyRemote('event', x)
    for (const x of r.event_projects) this.applyRemote('event_project', x)
    for (const x of r.people) this.applyRemote('person', x)
    for (const x of r.day_templates) this.applyRemote('day_template', x)
    for (const x of r.time_blocks) this.applyRemote('time_block', x)
    for (const x of r.day_records) this.applyRemote('day_record', x)
    for (const x of r.metrics) this.applyRemote('metric', x)
    for (const x of r.metric_entries) this.applyRemote('metric_entry', x)
    for (const x of r.goals) this.applyRemote('goal', x)
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
      } else if (c.kind === 'membership') {
        // What we can see changed (joined/left a group): reload everything and resubscribe.
        this.sync(0)
          .then(() => this.connect())
          .catch(() => {})
      } else if (c.kind === 'group') {
        const g = c.data as Group
        if (g.deleted_at || !g.members.some((m) => m.user_id === this.me?.id) && !this.me?.is_admin) this.groups.delete(g.id)
        else this.groups.set(g.id, g)
      } else if (c.kind === 'calendar_account') this.calendarAccount = c.data as CalendarAccountView | null
      else if (c.kind === 'namedays') {
        if (this.namedays) this.loadNamedays(true).catch(() => {})
      } else if (c.kind === 'occasion_template') {
        const t = c.data as OccasionTemplate
        this.occasionTemplates.set(t.kind, t)
      }
      else if (
        c.kind === 'focus_session' ||
        c.kind === 'series' ||
        c.kind === 'place' ||
        c.kind === 'workflow' ||
        c.kind === 'task_template' ||
        c.kind === 'calendar' ||
        c.kind === 'event' ||
        c.kind === 'event_project' ||
        c.kind === 'person' ||
        c.kind === 'day_template' ||
        c.kind === 'time_block' ||
        c.kind === 'day_record' ||
        c.kind === 'metric' ||
        c.kind === 'metric_entry' ||
        c.kind === 'goal'
      )
        this.applyRemote(c.kind, c.data as Entity)
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
      place: this.places,
      workflow: this.workflows,
      task_template: this.taskTemplates,
      group: this.groups,
      calendar: this.calendars,
      event: this.events,
      event_project: this.eventProjects,
      person: this.people,
      day_template: this.dayTemplates,
      time_block: this.timeBlocks,
      day_record: this.dayRecords,
      metric: this.metrics,
      metric_entry: this.metricEntries,
      goal: this.goals,
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

  /** An idea whose parent isn't one: where the "Idea" tag is shown (its subprojects follow it). */
  ideaRoot(p: Project): boolean {
    return p.status === 'idea' && !(p.parent_id && this.projects.get(p.parent_id)?.status === 'idea')
  }

  /** The task's project is an idea, so it asks for no attention (D-72). */
  inIdea(t: Task): boolean {
    return !!t.project_id && this.projects.get(t.project_id)?.status === 'idea'
  }

  /** Ready to work on: no open prerequisite, and any wait time after them is over. */
  isReady(t: Task): boolean {
    return !t.blocked && (!t.ready_at || t.ready_at <= new Date().toISOString())
  }

  /** Tasks that wait for `id`. */
  dependentsOf(id: string): Task[] {
    return [...this.tasks.values()].filter((t) => t.depends_on.includes(id))
  }

  /** Mirror the server: when a task's status changes, update what waits for it right away. */
  private refreshDependents(id: string) {
    for (const d of this.dependentsOf(id)) {
      if (d.status !== 'open') continue
      const blocked = isBlocked(d.depends_on, (x) => this.tasks.get(x)?.status)
      if (blocked === d.blocked) continue
      const ready_at = !blocked && d.wait_min ? new Date(Date.now() + d.wait_min * 60_000).toISOString() : null
      this.tasks.set(d.id, { ...d, blocked, ready_at })
    }
  }

  /** Set a task's prerequisites (the server refuses cycles). */
  setPrerequisites(id: string, deps: string[]) {
    const t = this.tasks.get(id)
    if (!t) return
    const blocked = isBlocked(deps, (x) => this.tasks.get(x)?.status)
    this.optimistic(
      [['task', id]],
      () => this.tasks.set(id, { ...t, depends_on: deps, blocked, ready_at: blocked ? null : t.ready_at }),
      async () => [['task', await api.patch<Task>(`/tasks/${id}`, { depends_on: deps })]],
    )
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
      .filter(
        (t) =>
          t.status === 'open' &&
          !plannedHere.has(t.id) &&
          !this.isUpcoming(t, date) &&
          this.atCurrentPlace(t) &&
          !t.blocked &&
          !t.waiting_since &&
          !this.inIdea(t),
      )
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
      owner_user_id: projectId && this.projects.get(projectId)?.owner_group_id ? null : (this.me?.id ?? null),
      owner_group_id: projectId ? (this.projects.get(projectId)?.owner_group_id ?? null) : null,
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
      event_id: input.event_id ?? null,
      place_id: input.place_id ?? (projectId ? (this.projects.get(projectId)?.default_place_id ?? null) : null),
      also_project_ids: [],
      depends_on: [],
      blocked: false,
      wait_min: null,
      ready_at: null,
      workflow_instance_id: null,
      workflow_step: null,
      workflow_steps: null,
      series_id: null,
      occurrence_key: null,
      occurrence_date: null,
      window_end: null,
      waiting_since: null,
      check_back_at: null,
      waiting_note: '',
      waiting_by: null,
      checklist: input.checklist ?? [],
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
          place_id: task.place_id,
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
      Object.assign(local, NOT_WAITING)
    }
    this.optimistic(
      [['task', id]],
      () => {
        this.tasks.set(id, local)
        if (patch.status && patch.status !== cur.status) this.refreshDependents(id)
      },
      async () => [['task', await api.patch<Task>(`/tasks/${id}`, patch)]],
    )
  }

  /** Tick or untick a checklist item; ticking the last one offers to complete the task (D-71). */
  tickItem(id: string, itemId: string) {
    const t = this.tasks.get(id)
    if (!t) return
    const checklist = t.checklist.map((i) => (i.id === itemId ? { ...i, done: !i.done } : i))
    this.updateTask(id, { checklist })
    const ticked = checklist.find((i) => i.id === itemId)?.done
    if (ticked && t.status === 'open' && checklist.every((i) => i.done))
      toast('All steps done', 'info', { label: 'Complete task', run: () => this.toggleDone(id) })
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

  /** A new project; `idea` makes it one that isn't started yet (D-72). Subprojects of an idea are ideas. */
  createProject(name: string, parentId: string | null = null, idea = false): string {
    const id = ulid()
    const ts = now()
    const parent = parentId ? this.projects.get(parentId) : null
    // A colour and shape that set it apart (D-74), as the server would pick them.
    const [color, shape] = parent
      ? [parent.color, pickSub(parent.shape, this.childProjects(parent.id, true).map((c) => c.shape))]
      : pickTop(this.childProjects(null, true).map((c) => [c.color, c.shape]))
    const p: Project = {
      id,
      owner_user_id: this.me?.id ?? null,
      owner_group_id: null,
      parent_id: parentId,
      name,
      color,
      shape,
      position: keyBetween(this.childProjects(parentId, true).at(-1)?.position, null),
      archived_at: null,
      default_place_id: null,
      description: '',
      status: idea || parent?.status === 'idea' ? 'idea' : 'active',
      created_at: ts,
      updated_at: ts,
      deleted_at: null,
      rev: 0,
    }
    this.optimistic(
      [['project', id]],
      () => this.projects.set(id, p),
      async () => [
        [
          'project',
          await api.post<Project>('/projects', {
            id,
            name,
            parent_id: parentId,
            color: p.color,
            shape: p.shape,
            position: p.position,
            status: p.status,
          }),
        ],
      ],
    )
    return id
  }

  updateProject(
    id: string,
    patch: {
      name?: string
      color?: string | null
      shape?: Shape
      position?: string
      archived?: boolean
      parent_id?: string | null
      default_place_id?: string | null
      owner_group_id?: string | null
      description?: string
      status?: 'active' | 'idea'
    },
  ) {
    const cur = this.projects.get(id)
    if (!cur) return
    const { archived, status, ...rest } = patch
    const ts = now()
    // Archiving and the idea status apply to the whole subtree (the server does the same).
    if ('owner_group_id' in patch) (rest as Record<string, unknown>).owner_user_id = patch.owner_group_id ? null : (this.me?.id ?? null)
    const touched = archived === undefined && status === undefined ? [id] : this.subtree(id)
    const updated = touched.map((pid) => {
      const p = pid === id ? { ...cur, ...rest } : { ...this.projects.get(pid)! }
      if (archived !== undefined) p.archived_at = archived ? (p.archived_at ?? ts) : null
      if (status !== undefined) p.status = status
      return { ...p, updated_at: ts }
    })
    // Becoming an idea takes its open tasks off the plan from today on.
    const unplanned =
      status === 'idea' && cur.status !== 'idea'
        ? [...this.entries.values()].filter((e) => {
            const t = this.tasks.get(e.task_id)
            return e.date >= this.today && t?.status === 'open' && !!t.project_id && touched.includes(t.project_id)
          })
        : []
    this.optimistic(
      [...touched.map((pid) => ['project', pid] as [Kind, string]), ...unplanned.map((e) => ['day_entry', e.id] as [Kind, string])],
      () => {
        updated.forEach((p) => this.projects.set(p.id, p))
        unplanned.forEach((e) => this.entries.delete(e.id))
      },
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
      () =>
        this.tasks.set(id, {
          ...cur,
          ...(inProgress ? {} : NOT_WAITING),
          started_at: inProgress ? (cur.started_at ?? now()) : null,
          updated_at: now(),
        }),
      async () => [['task', await api.patch<Task>(`/tasks/${id}`, { in_progress: inProgress })]],
    )
  }

  // ---- waiting for results (D-70) ---------------------------------------------

  /** Mark a task as waiting for results, with an optional check-back time (ISO) and note. */
  waitFor(id: string, checkBackAt: string | null, note?: string, extra: Pick<TaskPatch, 'position'> & { status?: 'open' } = {}) {
    const cur = this.tasks.get(id)
    if (!cur) return
    const ts = now()
    this.optimistic(
      [['task', id]],
      () =>
        this.tasks.set(id, {
          ...cur,
          ...(extra.status ? { status: 'open', completed_at: null, completed_by: null } : {}),
          ...(extra.position ? { position: extra.position } : {}),
          waiting_since: cur.waiting_since ?? ts,
          started_at: cur.started_at ?? ts,
          check_back_at: checkBackAt,
          waiting_note: (note ?? cur.waiting_note).trim(),
          waiting_by: this.me?.id ?? null,
          updated_at: ts,
        }),
      async () => [
        ['task', await api.patch<Task>(`/tasks/${id}`, { ...extra, waiting: true, check_back_at: checkBackAt, ...(note !== undefined ? { waiting_note: note } : {}) })],
      ],
    )
  }

  /** Stop waiting (or dismiss a due check-back): the task stays in progress. */
  stopWaiting(id: string) {
    const cur = this.tasks.get(id)
    if (!cur) return
    this.optimistic(
      [['task', id]],
      () => this.tasks.set(id, { ...cur, ...NOT_WAITING, updated_at: now() }),
      async () => [['task', await api.patch<Task>(`/tasks/${id}`, { waiting: false })]],
    )
  }

  /** Open tasks waiting for results, soonest check-back first (no time last). */
  waitingTasks(): Task[] {
    return [...this.tasks.values()]
      .filter((t) => isWaiting(t) && !this.inIdea(t))
      .sort((a, b) => {
        const x = a.check_back_at ?? '~'
        const y = b.check_back_at ?? '~'
        return x < y ? -1 : x > y ? 1 : byPosition(a, b)
      })
  }

  /** Waits whose check-back time has come, oldest first. */
  checkBacks(): Task[] {
    return [...this.tasks.values()]
      .filter((t) => checkBackDue(t) && !this.inIdea(t))
      .sort((a, b) => a.check_back_at!.localeCompare(b.check_back_at!))
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

  // ---- groups ----------------------------------------------------------------

  /** Groups I'm a member of (admins also see others in `groups`, for managing them). */
  myGroups(): Group[] {
    return [...this.groups.values()]
      .filter((g) => g.members.some((m) => m.user_id === this.me?.id))
      .sort((a, b) => a.name.localeCompare(b.name))
  }

  groupName(id: string | null | undefined): string {
    return (id && this.groups.get(id)?.name) || ''
  }

  /** A user's display name, from my groups' member lists. */
  personName(userId: string | null | undefined): string {
    if (!userId) return ''
    if (userId === this.me?.id) return 'you'
    for (const g of this.groups.values()) {
      const m = g.members.find((x) => x.user_id === userId)
      if (m) return m.display_name
    }
    return 'someone'
  }

  async groupAction<T>(fn: () => Promise<T>): Promise<T | null> {
    try {
      const out = await fn()
      if (out && typeof out === 'object' && 'members' in (out as object)) {
        const g = out as unknown as Group
        this.groups.set(g.id, g)
      }
      return out
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Something went wrong', 'error')
      return null
    }
  }
  createGroup = (name: string) => this.groupAction(() => api.post<Group>('/groups', { id: ulid(), name }))
  renameGroup = (id: string, name: string) => this.groupAction(() => api.patch<Group>(`/groups/${id}`, { name }))
  addMember = (id: string, userId: string, role: 'member' | 'owner' = 'member') =>
    this.groupAction(() => api.post<Group>(`/groups/${id}/members`, { user_id: userId, role }))
  removeMember = (id: string, userId: string) => this.groupAction(() => api.del(`/groups/${id}/members/${userId}`))
  deleteGroup = (id: string) => this.groupAction(() => api.del(`/groups/${id}`))
  directory = () => api.get<UserSummary[]>('/users/directory')

  /** Share a task without a project (`null` = just me). */
  shareTask(id: string, groupId: string | null) {
    const t = this.tasks.get(id)
    if (!t) return
    this.optimistic(
      [['task', id]],
      () => this.tasks.set(id, { ...t, owner_group_id: groupId, owner_user_id: groupId ? null : (this.me?.id ?? null) }),
      async () => [['task', await api.patch<Task>(`/tasks/${id}`, { owner_group_id: groupId })]],
    )
  }

  /** Share a top-level project and everything in it (`null` = just me). The server updates the rest. */
  shareProject(id: string, groupId: string | null) {
    this.updateProject(id, { owner_group_id: groupId })
  }

  // ---- default tasks (D-71) -----------------------------------------------------

  defaultTasks(): TaskTemplate[] {
    return [...this.taskTemplates.values()].sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' }))
  }

  /** Default tasks whose title starts with (or has a word starting with) what's typed. */
  matchDefaults(q: string, limit = 5): TaskTemplate[] {
    const s = q.trim().toLowerCase()
    if (s.length < 2) return []
    const starts = (t: TaskTemplate) => t.title.toLowerCase().startsWith(s)
    return this.defaultTasks()
      .filter((t) => starts(t) || t.title.toLowerCase().split(/\s+/).some((w) => w.startsWith(s)))
      .sort((a, b) => Number(starts(b)) - Number(starts(a)))
      .slice(0, limit)
  }

  async saveDefaultTask(id: string | null, body: Partial<Omit<TaskTemplate, 'id' | 'rev'>>): Promise<TaskTemplate | null> {
    try {
      const t = id
        ? await api.patch<TaskTemplate>(`/task-templates/${id}`, body)
        : await api.post<TaskTemplate>('/task-templates', { id: ulid(), ...body })
      this.applyRemote('task_template', t)
      return t
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Could not save the default task', 'error')
      return null
    }
  }

  deleteDefaultTask(id: string) {
    if (!this.taskTemplates.has(id)) return
    this.optimistic(
      [['task_template', id]],
      () => this.taskTemplates.delete(id),
      () => api.del(`/task-templates/${id}`),
    )
  }

  /** Save a task's details as a default task (its checklist unticked). */
  saveAsDefault(taskId: string) {
    const t = this.tasks.get(taskId)
    if (!t) return Promise.resolve(null)
    return this.saveDefaultTask(null, {
      title: t.title,
      notes: t.notes,
      checklist: t.checklist.map((i) => ({ ...i, done: false })),
      estimate_min: t.estimate_min,
      difficulty: t.difficulty,
      importance: t.importance,
      urgency: t.urgency,
      task_type_id: t.task_type_id === 'tt_carry_on' ? null : t.task_type_id,
      project_id: t.project_id,
      place_id: t.place_id,
    })
  }

  /** A new task from a default task; where it's added decides the project if the default has none. */
  createFromDefault(templateId: string, ctx: { day?: string; project_id?: string | null } = {}): string | null {
    const tpl = this.taskTemplates.get(templateId)
    if (!tpl) return null
    const projectId = tpl.project_id && this.projects.has(tpl.project_id) ? tpl.project_id : (ctx.project_id ?? null)
    const idea = !!projectId && this.projects.get(projectId)?.status === 'idea'
    return this.createTask({
      title: tpl.title,
      notes: tpl.notes,
      checklist: tpl.checklist.map((i) => ({ ...i, id: ulid(), done: false })),
      estimate_min: tpl.estimate_min,
      difficulty: tpl.difficulty,
      importance: tpl.importance,
      urgency: tpl.urgency,
      ...(tpl.task_type_id && this.taskTypes.has(tpl.task_type_id) ? { task_type_id: tpl.task_type_id } : {}),
      ...(tpl.place_id && this.places.has(tpl.place_id) ? { place_id: tpl.place_id } : {}),
      project_id: projectId,
      ...(ctx.day && !idea ? { day: ctx.day } : {}),
    })
  }

  // ---- workflows -------------------------------------------------------------

  async saveWorkflow(id: string | null, body: Record<string, unknown>): Promise<WorkflowTemplate | null> {
    try {
      const w = id
        ? await api.patch<WorkflowTemplate>(`/workflows/${id}`, body)
        : await api.post<WorkflowTemplate>('/workflows', { id: ulid(), ...body })
      this.applyRemote('workflow', w)
      return w
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Could not save the workflow', 'error')
      return null
    }
  }

  deleteWorkflow(id: string) {
    if (!this.workflows.has(id)) return
    this.optimistic(
      [['workflow', id]],
      () => this.workflows.delete(id),
      () => api.del(`/workflows/${id}`),
    )
  }

  /** Start a run: one chain per chosen variant. */
  async startWorkflow(id: string, variantIds: string[], day: string | null): Promise<Task[]> {
    try {
      const tasks = await api.post<Task[]>(`/workflows/${id}/start`, { variant_ids: variantIds, day })
      for (const t of tasks) this.applyRemote('task', t)
      return tasks
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Could not start the workflow', 'error')
      return []
    }
  }

  /** The steps of a workflow run, in order. */
  workflowRun(instanceId: string): Task[] {
    return [...this.tasks.values()]
      .filter((t) => t.workflow_instance_id === instanceId)
      .sort((a, b) => (a.workflow_step ?? 0) - (b.workflow_step ?? 0))
  }

  /**
   * Move one routine occurrence to another day (e.g. a workout clashes with something):
   * it becomes due that day, keeps its time if it had one, and isn't missed on its
   * original day.
   */
  moveOccurrence(id: string, date: string) {
    const t = this.tasks.get(id)
    if (!t) return
    const entry = this.entryForTask(id)
    this.updateTask(id, { due_date: date })
    if (entry) this.plan(id, date, { startTime: entry.start_time })
  }

  // ---- calendar -------------------------------------------------------------

  /** Calendar events on a local day (all-day ones and timed ones clipped to the day). */
  dayEvents(date: string) {
    return eventsOn(date, this.events.values(), this.me?.timezone ?? 'UTC', (id) => this.calendars.get(id))
  }

  /** Busy calendar time on a day, for free-time math. */
  dayBusy(date: string): [string, number][] {
    const ev = this.dayEvents(date)
    const allDay = ev.allDay.some((d) => this.calendars.get(d.event.calendar_id)?.all_day_busy)
    return allDay ? [['00:00', 1440]] : busyIntervals(ev.timed)
  }

  /** The project link of an event (all instances of a recurring one), if any (D-68). */
  eventLink(ev: CalendarEvent): EventProject | undefined {
    for (const l of this.eventProjects.values()) if (l.calendar_id === ev.calendar_id && l.uid === ev.uid) return l
    return undefined
  }

  /** The project an event belongs to, if it's still visible. */
  eventProject(ev: CalendarEvent): Project | undefined {
    const l = this.eventLink(ev)
    return l ? this.projects.get(l.project_id) : undefined
  }

  /** Put an event (every instance) into a project, or take it out (`null`). */
  setEventProject(ev: CalendarEvent, projectId: string | null) {
    const cur = this.eventLink(ev)
    if (!cur && !projectId) return
    const id = cur?.id ?? ulid()
    const ts = now()
    return this.optimistic(
      [['event_project', id]],
      () => {
        if (!projectId) this.eventProjects.delete(id)
        else
          this.eventProjects.set(id, {
            id,
            user_id: this.me?.id ?? '',
            calendar_id: ev.calendar_id,
            uid: ev.uid,
            project_id: projectId,
            created_at: cur?.created_at ?? ts,
            updated_at: ts,
            deleted_at: null,
            rev: cur?.rev ?? 0,
          })
      },
      async () => [
        [
          'event_project',
          await api.put<EventProject>('/calendar/event-projects', {
            id,
            calendar_id: ev.calendar_id,
            uid: ev.uid,
            project_id: projectId,
          }),
        ],
      ],
    )
  }

  /** Todos attached to an event instance, open ones first. */
  eventTasks(eventId: string): Task[] {
    return [...this.tasks.values()]
      .filter((t) => t.event_id === eventId && !t.deleted_at)
      .sort((a, b) => Number(a.status !== 'open') - Number(b.status !== 'open') || (a.position < b.position ? -1 : 1))
  }

  /** The local date an event starts on. */
  eventDate(ev: CalendarEvent): string {
    return ev.all_day ? ev.start_date! : zoned(ev.start_at!, this.me?.timezone ?? 'UTC').date
  }

  /** Upcoming events of a project (from today on), soonest first. */
  projectEvents(projectId: string, limit = 5): CalendarEvent[] {
    const uids = new Set(
      [...this.eventProjects.values()].filter((l) => l.project_id === projectId).map((l) => `${l.calendar_id}|${l.uid}`),
    )
    if (!uids.size) return []
    const start = (e: CalendarEvent) => (e.all_day ? e.start_date! : e.start_at!)
    return [...this.events.values()]
      .filter((e) => !e.deleted_at && uids.has(`${e.calendar_id}|${e.uid}`) && this.eventDate(e) >= this.today)
      .filter((e) => this.calendars.get(e.calendar_id)?.enabled !== false)
      .sort((a, b) => (this.eventDate(a) + start(a) < this.eventDate(b) + start(b) ? -1 : 1))
      .slice(0, limit)
  }

  // ---- blocks, templates and conflicts --------------------------------------

  blocksOn(date: string): TimeBlock[] {
    return [...this.timeBlocks.values()]
      .filter((b) => b.date === date)
      .sort((a, b) => (a.start_time < b.start_time ? -1 : a.start_time > b.start_time ? 1 : 0))
  }

  templateList(): DayTemplate[] {
    return [...this.dayTemplates.values()].sort(byPosition)
  }

  /** Everything that takes time on a day, for conflict checks (ids: entry, event, block). */
  dayItems(date: string): Item[] {
    const m = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3, 5))
    const items: Item[] = []
    for (const e of this.dayEntries(date)) {
      const t = this.tasks.get(e.task_id)
      // Waiting for results takes no time (D-70).
      if (!e.start_time || !t || t.status !== 'open' || t.waiting_since) continue
      const start = m(e.start_time)
      items.push({ id: e.id, kind: 'task', start, end: Math.min(1440, start + (e.duration_min ?? t.estimate_min ?? 30)) })
    }
    const ev = this.dayEvents(date)
    for (const d of ev.timed) if (d.event.busy) items.push({ id: d.event.id, kind: 'event', start: d.start, end: d.start + d.dur })
    for (const d of ev.allDay)
      if (this.calendars.get(d.event.calendar_id)?.all_day_busy) items.push({ id: d.event.id, kind: 'event', start: 0, end: 1440 })
    for (const b of this.blocksOn(date)) items.push({ id: b.id, kind: 'block', start: m(b.start_time), end: m(b.end_time) })
    return items
  }

  dayConflicts(date: string): Conflict[] {
    return findConflicts(this.dayItems(date))
  }

  /** What an item of `dayItems` is called. */
  itemTitle(id: string): string {
    const e = this.entries.get(id)
    if (e) return this.tasks.get(e.task_id)?.title ?? ''
    return this.events.get(id)?.title ?? this.timeBlocks.get(id)?.title ?? ''
  }

  /** Quick fix: move a scheduled entry to the next free time of the day (null = no room). */
  moveToFreeTime(entryId: string): string | null {
    const e = this.entries.get(entryId)
    if (!e?.start_time || !this.me) return null
    const items = this.dayItems(e.date)
    const self = items.find((i) => i.id === entryId)
    if (!self) return null
    const busy = items.filter((i) => i.kind !== 'block' && i.id !== entryId).map((i): [number, number] => [i.start, i.end])
    const m = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3, 5))
    const ws = m(this.me.day_window_start)
    const we = m(this.me.day_window_end) > ws ? m(this.me.day_window_end) : 1440
    const slot = nextFreeSlot(busy, self.end - self.start, self.start, [ws, we])
    if (slot === null) return null
    const time = `${String(Math.floor(slot / 60)).padStart(2, '0')}:${String(slot % 60).padStart(2, '0')}`
    this.updateEntry(entryId, { start_time: time })
    return time
  }

  /** Quick fix: end the entry where the next overlapping item starts (false = can't). */
  shortenToFit(entryId: string): boolean {
    const e = this.entries.get(entryId)
    if (!e?.start_time) return false
    const items = this.dayItems(e.date)
    const self = items.find((i) => i.id === entryId)
    if (!self) return false
    const next = items
      .filter((i) => i.id !== entryId && i.kind !== 'block' && i.start > self.start && i.start < self.end)
      .sort((a, b) => a.start - b.start)[0]
    const blocking = items.some((i) => i.id !== entryId && i.kind !== 'block' && i.start <= self.start && i.end > self.start)
    if (!next || blocking) return false
    this.updateEntry(entryId, { duration_min: next.start - self.start })
    return true
  }

  createTemplate(name: string, weekdays: number[], blocks: TemplateBlock[]) {
    const id = ulid()
    const ts = now()
    const t: DayTemplate = {
      id,
      owner_user_id: this.me?.id ?? '',
      name,
      weekdays,
      blocks,
      position: keyBetween(this.templateList().at(-1)?.position, null),
      created_at: ts,
      updated_at: ts,
      deleted_at: null,
      rev: 0,
    }
    return this.optimistic(
      [['day_template', id]],
      () => this.dayTemplates.set(id, t),
      async () => [['day_template', await api.post<DayTemplate>('/day-templates', { id, name, weekdays, blocks })]],
    )
  }

  updateTemplate(id: string, patch: Partial<Pick<DayTemplate, 'name' | 'weekdays' | 'blocks'>>) {
    const cur = this.dayTemplates.get(id)
    if (!cur) return
    return this.optimistic(
      [['day_template', id]],
      () => this.dayTemplates.set(id, { ...cur, ...patch }),
      async () => [['day_template', await api.patch<DayTemplate>(`/day-templates/${id}`, patch)]],
    )
  }

  deleteTemplate(id: string) {
    if (!this.dayTemplates.has(id)) return
    return this.optimistic(
      [['day_template', id]],
      () => this.dayTemplates.delete(id),
      () => api.del(`/day-templates/${id}`),
    )
  }

  async applyTemplate(date: string, templateId: string | null) {
    try {
      await api.post(`/days/${date}/apply-template`, { template_id: templateId })
      await this.sync()
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Could not reach the server', 'error')
    }
  }

  createBlock(date: string, b: { title: string; start_time: string; end_time: string; energy: string | null }) {
    const id = ulid()
    const ts = now()
    const block: TimeBlock = { id, user_id: this.me?.id ?? '', date, template_id: null, created_at: ts, updated_at: ts, deleted_at: null, rev: 0, ...b }
    return this.optimistic(
      [['time_block', id]],
      () => this.timeBlocks.set(id, block),
      async () => [['time_block', await api.post<TimeBlock>(`/days/${date}/blocks`, { id, ...b })]],
    )
  }

  updateBlock(id: string, patch: Partial<Pick<TimeBlock, 'title' | 'start_time' | 'end_time' | 'energy'>>) {
    const cur = this.timeBlocks.get(id)
    if (!cur) return
    return this.optimistic(
      [['time_block', id]],
      () => this.timeBlocks.set(id, { ...cur, ...patch }),
      async () => [['time_block', await api.patch<TimeBlock>(`/time-blocks/${id}`, patch)]],
    )
  }

  deleteBlock(id: string) {
    if (!this.timeBlocks.has(id)) return
    return this.optimistic(
      [['time_block', id]],
      () => this.timeBlocks.delete(id),
      () => api.del(`/time-blocks/${id}`),
    )
  }

  suggestPlan(date: string) {
    return api.post<SuggestedPlan>(`/days/${date}/suggest`, {})
  }

  updateCalendarAllDay(id: string, all_day_busy: boolean) {
    const cur = this.calendars.get(id)
    if (!cur) return
    return this.optimistic(
      [['calendar', id]],
      () => this.calendars.set(id, { ...cur, all_day_busy }),
      async () => [['calendar', await api.patch<Calendar>(`/calendars/${id}`, { all_day_busy })]],
    )
  }

  /** Connect or update the calendar account; the server syncs before answering. */
  async saveCalendarAccount(input: { url: string; username: string; password?: string }) {
    this.calendarAccount = await api.put<CalendarAccountView>('/calendar/account', input)
    await this.sync()
    return this.calendarAccount
  }

  testCalendar(input: { url: string; username: string; password?: string }) {
    return api.post<{ ok: boolean; calendars: string[]; error: string | null }>('/calendar/test', input)
  }

  async syncCalendar() {
    this.calendarAccount = await api.post<CalendarAccountView>('/calendar/sync', {})
    await this.sync()
  }

  async disconnectCalendar() {
    await api.del('/calendar/account')
    this.calendarAccount = null
    await this.sync()
  }

  updateCalendar(id: string, patch: { enabled?: boolean; user_color?: string | null }) {
    const cur = this.calendars.get(id)
    if (!cur) return
    return this.optimistic(
      [['calendar', id]],
      () => this.calendars.set(id, { ...cur, ...patch }),
      async () => [['calendar', await api.patch<Calendar>(`/calendars/${id}`, patch)]],
    )
  }

  // ---- tracking ---------------------------------------------------------------

  recordFor(date: string): DayRecord | undefined {
    for (const r of this.dayRecords.values()) if (r.date === date) return r
  }

  /** Save reflection fields (the server keeps fields left out). */
  async saveRecord(date: string, patch: Partial<Pick<DayRecord, 'journal' | 'went_well' | 'went_badly' | 'tomorrow'>>) {
    const r = await api.put<DayRecord>(`/days/${date}/record`, patch)
    this.applyRemote('day_record', r)
    return r
  }

  metricList(includeArchived = false): MetricDefinition[] {
    return [...this.metrics.values()].filter((m) => includeArchived || !m.archived).sort(byPosition)
  }

  metricByKey(key: string): MetricDefinition | undefined {
    for (const m of this.metrics.values()) if (m.key === key) return m
  }

  entriesOf(metricId: string): MetricEntry[] {
    return [...this.metricEntries.values()].filter((e) => e.metric_id === metricId)
  }

  /** A metric's value per day, its entries combined. */
  metricDaily(metricId: string): Map<string, number> {
    const m = this.metrics.get(metricId)
    return daily(this.entriesOf(metricId), (m?.aggregate ?? 'latest') as 'latest')
  }

  logMetric(metricId: string, value: number, date = this.today) {
    const id = ulid()
    const ts = now()
    const e: MetricEntry = { id, user_id: this.me?.id ?? '', metric_id: metricId, date, at: ts, value, note: '', created_at: ts, updated_at: ts, deleted_at: null, rev: 0 }
    return this.optimistic(
      [['metric_entry', id]],
      () => this.metricEntries.set(id, e),
      async () => [['metric_entry', await api.post<MetricEntry>(`/metrics/${metricId}/entries`, { id, date, value })]],
    )
  }

  deleteMetricEntry(id: string) {
    if (!this.metricEntries.has(id)) return
    return this.optimistic(
      [['metric_entry', id]],
      () => this.metricEntries.delete(id),
      () => api.del(`/metric-entries/${id}`),
    )
  }

  async createMetric(input: { name: string; kind: string; unit?: string; scale_min?: number; scale_max?: number; aggregate?: string; reminder_time?: string | null }) {
    try {
      const m = await api.post<MetricDefinition>('/metrics', input)
      this.applyRemote('metric', m)
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Could not reach the server', 'error')
    }
  }

  updateMetric(id: string, patch: Partial<Pick<MetricDefinition, 'name' | 'unit' | 'aggregate' | 'reminder_time' | 'archived'>>) {
    const cur = this.metrics.get(id)
    if (!cur) return
    return this.optimistic(
      [['metric', id]],
      () => this.metrics.set(id, { ...cur, ...patch }),
      async () => [['metric', await api.patch<MetricDefinition>(`/metrics/${id}`, patch)]],
    )
  }

  deleteMetric(id: string) {
    if (!this.metrics.has(id)) return
    return this.optimistic(
      [['metric', id]],
      () => this.metrics.delete(id),
      () => api.del(`/metrics/${id}`),
    )
  }

  // ---- goals -----------------------------------------------------------------

  goalList(): Goal[] {
    return [...this.goals.values()].sort(byPosition)
  }

  async loadGoalProgress() {
    try {
      const ps = await api.get<GoalProgress[]>('/goals/progress')
      this.goalProgress.clear()
      for (const p of ps) this.goalProgress.set(p.goal_id, p)
    } catch {
      /* offline */
    }
  }

  /** Whether the periodic goals review is due (and there's something to review). */
  goalReviewDue(): boolean {
    if (!this.me || ![...this.goals.values()].some((g) => g.status === 'active')) return false
    return reviewDue(this.me.review_cadence as Cadence, this.me.last_review_date, this.today, this.me.week_start)
  }

  createGoal(title: string, groupId: string | null = null) {
    const id = ulid()
    const ts = now()
    const g: Goal = {
      id,
      owner_user_id: groupId ? null : (this.me?.id ?? null),
      owner_group_id: groupId,
      title,
      description: '',
      target_date: null,
      status: 'active',
      progress_override: null,
      milestones: [],
      project_ids: [],
      task_ids: [],
      position: keyBetween(this.goalList().at(-1)?.position, null),
      created_at: ts,
      updated_at: ts,
      deleted_at: null,
      rev: 0,
    }
    return this.optimistic(
      [['goal', id]],
      () => this.goals.set(id, g),
      async () => [['goal', await api.post<Goal>('/goals', { id, title, owner_group_id: groupId })]],
    ).then(() => this.loadGoalProgress())
  }

  updateGoal(
    id: string,
    patch: Partial<Pick<Goal, 'title' | 'description' | 'target_date' | 'status' | 'progress_override' | 'milestones' | 'project_ids' | 'task_ids' | 'owner_group_id'>>,
  ) {
    const cur = this.goals.get(id)
    if (!cur) return
    const local = { ...cur, ...patch }
    if ('owner_group_id' in patch) local.owner_user_id = patch.owner_group_id ? null : (this.me?.id ?? null)
    return this.optimistic(
      [['goal', id]],
      () => this.goals.set(id, local),
      async () => [['goal', await api.patch<Goal>(`/goals/${id}`, patch)]],
    ).then(() => this.loadGoalProgress())
  }

  deleteGoal(id: string) {
    if (!this.goals.has(id)) return
    return this.optimistic(
      [['goal', id]],
      () => this.goals.delete(id),
      () => api.del(`/goals/${id}`),
    )
  }

  async submitReview(notes: { goal_id: string; note: string }[]) {
    this.me = await api.post<Me>('/goals/review', { notes })
  }

  // ---- occasions -------------------------------------------------------------

  /** Load the shared nameday calendar (once; `force` reloads it). */
  loadNamedays(force = false): Promise<void> {
    if (this.namedays && !force) return Promise.resolve()
    if (!this.namedaysLoading || force)
      this.namedaysLoading = api
        .get<NamedayCalendar>('/namedays')
        .then((c) => {
          const byDate = new Map<string, string[]>()
          for (const d of c.days) byDate.set(`${String(d.month).padStart(2, '0')}-${String(d.day).padStart(2, '0')}`, d.names)
          this.namedays = { label: c.source?.label ?? null, byDate }
        })
        .finally(() => (this.namedaysLoading = null))
    return this.namedaysLoading
  }

  /** Names with a nameday on a date (`YYYY-MM-DD`); 29 Feb names show on the 28th in other years. */
  namedaysOn(date: string): string[] {
    const byDate = this.namedays?.byDate
    if (!byDate) return []
    const md = date.slice(5)
    const names = [...(byDate.get(md) ?? [])]
    const y = Number(date.slice(0, 4))
    const leap = (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0
    if (md === '02-28' && !leap) names.push(...(byDate.get('02-29') ?? []))
    return names
  }

  /** Your people whose nameday or birthday is on `date`. */
  occasionsOn(date: string): { person: Person; kind: 'nameday' | 'birthday' }[] {
    const today = new Set(this.namedaysOn(date).map(fold))
    const md = date.slice(5)
    const out: { person: Person; kind: 'nameday' | 'birthday' }[] = []
    for (const p of this.people.values()) {
      if (p.nameday_name && today.has(fold(p.nameday_name))) out.push({ person: p, kind: 'nameday' })
      if (p.birthday && p.birthday.slice(-5) === md) out.push({ person: p, kind: 'birthday' })
    }
    return out
  }

  searchNames(q: string) {
    return api.get<NameMatch[]>(`/namedays/search?q=${encodeURIComponent(q)}`)
  }

  peopleList(): Person[] {
    return [...this.people.values()].sort((a, b) => a.name.localeCompare(b.name))
  }

  createPerson(input: { name: string; nameday_name?: string | null; birthday?: string | null }) {
    const id = ulid()
    const ts = now()
    const p: Person = {
      id,
      owner_user_id: this.me?.id ?? '',
      name: input.name,
      nameday_name: input.nameday_name ?? null,
      birthday: input.birthday ?? null,
      created_at: ts,
      updated_at: ts,
      deleted_at: null,
      rev: 0,
    }
    return this.optimistic(
      [['person', id]],
      () => this.people.set(id, p),
      async () => [['person', await api.post<Person>('/people', { id, ...input })]],
    )
  }

  updatePerson(id: string, patch: Partial<Pick<Person, 'name' | 'nameday_name' | 'birthday'>>) {
    const cur = this.people.get(id)
    if (!cur) return
    return this.optimistic(
      [['person', id]],
      () => this.people.set(id, { ...cur, ...patch }),
      async () => [['person', await api.patch<Person>(`/people/${id}`, patch)]],
    )
  }

  deletePerson(id: string) {
    if (!this.people.has(id)) return
    return this.optimistic(
      [['person', id]],
      () => this.people.delete(id),
      () => api.del(`/people/${id}`),
    )
  }

  async updateOccasionTemplate(kind: string, patch: { enabled?: boolean; steps?: OccasionStep[] }) {
    try {
      const t = await api.put<OccasionTemplate>(`/occasion-templates/${kind}`, patch)
      this.occasionTemplates.set(t.kind, t)
    } catch (e) {
      toast(e instanceof ApiError ? e.message : 'Could not reach the server', 'error')
      throw e
    }
  }

  /** Admins: download the official nameday list, or upload one. */
  async reloadNamedays(upload?: { text: string; label: string }) {
    await api.post('/namedays', upload ?? {})
    await this.loadNamedays(true)
  }

  // ---- places ----------------------------------------------------------------

  placeList(): Place[] {
    return [...this.places.values()].sort(byPosition)
  }

  /** Can the task be done where this device is? (No place set, or the task has none, or it matches.) */
  atCurrentPlace(t: Task): boolean {
    return !this.currentPlace || !t.place_id || t.place_id === this.currentPlace || !this.places.has(t.place_id)
  }

  setCurrentPlace(id: string) {
    this.currentPlace = id
    writeLocal('sl.place', id)
  }

  setUseGps(on: boolean) {
    this.useGps = on
    writeLocal('sl.gps', on ? '1' : '')
  }

  createPlace(name: string, coords?: { lat: number; lon: number }): string {
    const id = ulid()
    const ts = now()
    const p: Place = {
      id,
      owner_user_id: this.me?.id ?? null,
      owner_group_id: null,
      name,
      lat: coords?.lat ?? null,
      lon: coords?.lon ?? null,
      radius_m: 200,
      position: keyBetween(this.placeList().at(-1)?.position, null),
      created_at: ts,
      updated_at: ts,
      deleted_at: null,
      rev: 0,
    }
    this.optimistic(
      [['place', id]],
      () => this.places.set(id, p),
      async () => [['place', await api.post<Place>('/places', { id, name, lat: p.lat, lon: p.lon, radius_m: p.radius_m })]],
    )
    return id
  }

  updatePlace(id: string, patch: Partial<Pick<Place, 'name' | 'lat' | 'lon' | 'radius_m'>>) {
    const cur = this.places.get(id)
    if (!cur) return
    this.optimistic(
      [['place', id]],
      () => this.places.set(id, { ...cur, ...patch, updated_at: now() }),
      async () => [['place', await api.patch<Place>(`/places/${id}`, patch)]],
    )
  }

  deletePlace(id: string) {
    const cur = this.places.get(id)
    if (!cur) return
    if (this.currentPlace === id) this.setCurrentPlace('')
    this.optimistic(
      [['place', id]],
      () => this.places.delete(id),
      () => api.del(`/places/${id}`),
    )
  }

  // ---- routines ------------------------------------------------------------

  /** Routines that are still running (not ended before today). */
  activeSeries(): Series[] {
    return [...this.series.values()]
      .filter((s) => !s.until || s.until >= this.today)
      .sort((a, b) => a.title.localeCompare(b.title))
  }

  /** The first version of a routine (versions link back via `split_from` on schedule changes). */
  private seriesRoot(id: string): string {
    let cur = id
    for (let i = 0; i < 50; i++) {
      const prev = this.series.get(cur)?.split_from
      if (!prev || !this.series.has(prev)) break
      cur = prev
    }
    return cur
  }

  /** All versions of the routine `id` belongs to. */
  seriesFamily(id: string): Set<string> {
    const root = this.seriesRoot(id)
    const out = new Set([id, root])
    for (const s of this.series.values()) if (this.seriesRoot(s.id) === root) out.add(s.id)
    return out
  }

  /** Done/total of a flexible routine's occurrences in the window that contains `day` (all versions). */
  windowProgress(seriesId: string, day: string): { done: number; total: number } {
    const family = this.seriesFamily(seriesId)
    let done = 0
    let total = 0
    for (const t of this.tasks.values())
      if (t.series_id && family.has(t.series_id) && t.window_end && t.occurrence_date! <= day && day <= t.window_end) {
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
        | 'unit_system'
        | 'review_cadence'
        | 'notify_off'
        | 'ntfy_url'
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
