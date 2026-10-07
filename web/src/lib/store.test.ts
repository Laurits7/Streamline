// Store behaviour that makes the UI feel instant and stay correct: optimistic
// updates, rollback on failure, merging live changes, and project-tree rules.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { CalendarEvent } from './api/types/CalendarEvent'
import type { DayEntry } from './api/types/DayEntry'
import type { Project } from './api/types/Project'
import type { SyncResponse } from './api/types/SyncResponse'
import type { Task } from './api/types/Task'
import { store } from './store.svelte'
import { toasts } from './toast.svelte'

const TS = '2026-10-06T08:00:00.000Z'
const ME = {
  id: 'U1', username: 'me', display_name: 'Me', is_admin: true, timezone: 'UTC', day_end: '04:00', locale: '', week_start: 1,
  plan_mode: 'evening' as const, plan_time_evening: '21:00', plan_time_morning: '07:30', day_window_start: '08:00', day_window_end: '22:00',
  prefs: {}, focus_work_min: 25, focus_short_break_min: 5, focus_long_break_min: 15, focus_long_every: 4, unit_system: 'metric' as const, review_cadence: 'weekly' as const, last_review_date: null, notify_off: [], ntfy_url: null,
}

const task = (id: string, extra: Partial<Task> = {}): Task => ({
  id,
  owner_user_id: 'U1',
  owner_group_id: null,
  assignee_user_id: null,
  project_id: null,
  title: id,
  notes: '',
  status: 'open',
  position: 'V',
  due_date: null,
  estimate_min: null,
  difficulty: null,
  importance: null,
  urgency: null,
  actual_min: 0,
  task_type_id: 'tt_carry_on',
  carry_count: 0,
  started_at: null,
  completed_at: null,
  completed_by: null,
  ext_source: null,
  ext_id: null,
  ext_url: null,
  place_id: null,
  event_id: null,
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
  checklist: [],
  created_at: TS,
  updated_at: TS,
  deleted_at: null,
  rev: 1,
  ...extra,
})

const project = (id: string, extra: Partial<Project> = {}): Project => ({
  id,
  owner_user_id: 'U1',
  owner_group_id: null,
  parent_id: null,
  name: id,
  color: null,
  shape: 'circle',
  position: 'V',
  archived_at: null,
  default_place_id: null,
  description: '',
  status: 'active',
  created_at: TS,
  updated_at: TS,
  deleted_at: null,
  rev: 1,
  ...extra,
})

const entry = (id: string, taskId: string, date: string, extra: Partial<DayEntry> = {}): DayEntry => ({
  id,
  user_id: 'U1',
  date,
  task_id: taskId,
  position: 'V',
  start_time: null,
  duration_min: null,
  created_at: TS,
  updated_at: TS,
  deleted_at: null,
  rev: 1,
  ...extra,
})

type Handler = (method: string, path: string, body: unknown) => unknown | Promise<unknown>
let calls: { method: string; path: string; body: unknown }[] = []

/** Route fetch() through `handler`. Return a value for 200 JSON, or throw `{status}` for an error. */
function mockApi(handler: Handler) {
  calls = []
  vi.stubGlobal('fetch', async (url: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    const path = url.replace('/api/v1', '')
    const body = init?.body ? JSON.parse(init.body as string) : undefined
    calls.push({ method, path, body })
    try {
      const out = await handler(method, path, body)
      return new Response(out === undefined ? null : JSON.stringify(out), { status: out === undefined ? 204 : 200 })
    } catch (e) {
      const status = (e as { status?: number }).status ?? 500
      return new Response(JSON.stringify({ title: 'Error', status, detail: `failed with ${status}` }), { status })
    }
  })
}

function syncResponse(data: Partial<SyncResponse> = {}): SyncResponse {
  return { rev: 10, full: true, me: ME, today: '2026-10-06', task_types: [], projects: [], tasks: [], day_entries: [], day_plans: [],
    focus_timer: { task_id: null, phase: 'idle', running_since_ms: null, elapsed_ms: 0, length_min: 0, cycle_done: 0, rev: 0 },
    focus_sessions: [], series: [], places: [], workflows: [], task_templates: [], groups: [], calendar_account: null, calendars: [], events: [], event_projects: [], people: [], occasion_templates: [], day_templates: [], time_blocks: [], day_records: [], metrics: [], metric_entries: [], health_days: [], goals: [], server_now: Date.now(), ...data }
}

/** Load the store with a full sync of the given data. */
async function load(data: Partial<SyncResponse> = {}) {
  mockApi(() => syncResponse(data))
  await store.sync(0)
}

const tick = () => new Promise((r) => setTimeout(r, 0))
const remote = (kind: string, e: unknown) => (store as unknown as { applyRemote: (k: string, e: unknown) => void }).applyRemote(kind, e)

beforeEach(() => {
  store.stop()
  toasts.splice(0)
})
afterEach(() => vi.unstubAllGlobals())

describe('sync', () => {
  it('loads everything and tracks the latest rev', async () => {
    await load({ tasks: [task('T1', { rev: 12 })], projects: [project('P1')] })
    expect(store.me?.id).toBe('U1')
    expect(store.tasks.get('T1')?.title).toBe('T1')
    expect(store.projects.size).toBe(1)
    expect(store.rev).toBe(12)
  })

  it('applies deletions from a delta sync', async () => {
    await load({ tasks: [task('T1')] })
    mockApi(() => syncResponse({ full: false, rev: 20, tasks: [task('T1', { rev: 20, deleted_at: TS })] }))
    await store.sync()
    expect(calls[0].path).toBe('/sync?since=10')
    expect(store.tasks.has('T1')).toBe(false)
  })
})

describe('optimistic updates', () => {
  it('shows a new task before the server answers, then adopts the server copy', async () => {
    await load()
    let release!: (v: unknown) => void
    mockApi((_m, _p, body) => new Promise((r) => (release = () => r({ ...task((body as Task).id), title: 'Buy milk', rev: 11 }))))
    const id = store.createTask({ title: 'Buy milk' })
    expect(store.tasks.get(id)?.title).toBe('Buy milk')
    expect(store.tasks.get(id)?.rev).toBe(0)
    expect(calls[0].body).toMatchObject({ id, title: 'Buy milk' })
    release(undefined)
    await tick()
    await tick()
    expect(store.tasks.get(id)?.rev).toBe(11)
  })

  it('rolls back and reports when the server rejects a change', async () => {
    await load({ tasks: [task('T1', { title: 'Original' })] })
    mockApi((method) => {
      if (method === 'PATCH') throw { status: 400 }
      return syncResponse({ full: false, tasks: [] })
    })
    store.updateTask('T1', { title: 'Changed' })
    expect(store.tasks.get('T1')?.title).toBe('Changed')
    await tick()
    await tick()
    expect(store.tasks.get('T1')?.title).toBe('Original')
    expect(toasts.at(-1)).toMatchObject({ kind: 'error', text: 'failed with 400' })
  })

  it('completing sets who and when locally', async () => {
    await load({ tasks: [task('T1')] })
    mockApi(() => new Promise(() => {}))
    store.updateTask('T1', { status: 'done' })
    expect(store.tasks.get('T1')).toMatchObject({ status: 'done', completed_by: 'U1' })
    expect(store.tasks.get('T1')?.completed_at).toBeTruthy()
  })
})

describe('live changes', () => {
  it('ignores stale events and applies newer ones and tombstones', async () => {
    await load({ tasks: [task('T1', { title: 'v5', rev: 5 })] })
    remote('task', task('T1', { title: 'v4', rev: 4 }))
    expect(store.tasks.get('T1')?.title).toBe('v5')
    remote('task', task('T1', { title: 'v6', rev: 6 }))
    expect(store.tasks.get('T1')?.title).toBe('v6')
    remote('task', task('T1', { rev: 7, deleted_at: TS }))
    expect(store.tasks.has('T1')).toBe(false)
  })

  it("doesn't let an echo overwrite an edit that is still in flight", async () => {
    await load({ tasks: [task('T1', { title: 'Old', rev: 5 })] })
    mockApi(() => new Promise(() => {}))
    store.updateTask('T1', { title: 'Mine' })
    remote('task', task('T1', { title: 'Old', rev: 6 }))
    expect(store.tasks.get('T1')?.title).toBe('Mine')
  })
})

describe('day plan', () => {
  it('a task has one entry: planning it on another day moves it', async () => {
    await load({ tasks: [task('T1')], day_entries: [entry('E1', 'T1', '2026-10-06', { start_time: '09:00' })] })
    mockApi(() => new Promise(() => {}))
    store.plan('T1', '2026-10-07')
    const entries = [...store.entries.values()]
    expect(entries).toHaveLength(1)
    expect(entries[0]).toMatchObject({ id: 'E1', date: '2026-10-07', start_time: null })
    expect(calls[0]).toMatchObject({ method: 'POST', path: '/days/2026-10-07/entries' })
  })

  it('keeps the time when re-planned on the same day unless told otherwise', async () => {
    await load({ tasks: [task('T1')], day_entries: [entry('E1', 'T1', '2026-10-06', { start_time: '09:00' })] })
    mockApi(() => new Promise(() => {}))
    store.plan('T1', '2026-10-06', { position: 'a' })
    expect(store.entries.get('E1')).toMatchObject({ start_time: '09:00', position: 'a' })
    store.plan('T1', '2026-10-06', { startTime: null })
    expect(store.entries.get('E1')?.start_time).toBeNull()
  })
})

describe('project tree', () => {
  const tree = () =>
    load({
      projects: [
        project('Paper'),
        project('Writing', { parent_id: 'Paper' }),
        project('Draft', { parent_id: 'Writing' }),
        project('Garden', { position: 'k' }),
      ],
      tasks: [task('T1', { project_id: 'Draft' }), task('T2', { project_id: 'Paper' })],
    })

  it('walks the tree depth-first with depths and paths', async () => {
    await tree()
    expect(store.projectTree().map((n) => `${n.depth}:${n.project.id}`)).toEqual(['0:Paper', '1:Writing', '2:Draft', '0:Garden'])
    expect(store.projectTree((id) => id === 'Writing').map((n) => n.project.id)).toEqual(['Paper', 'Writing', 'Garden'])
    expect(store.projectPath('Draft')).toBe('Paper › Writing › Draft')
    expect(store.openCountDeep('Paper')).toBe(2)
  })

  it('refuses to move a project into its own subtree', async () => {
    await tree()
    mockApi(() => new Promise(() => {}))
    expect(store.canMoveProject('Paper', 'Draft')).toBe(false)
    store.moveProject('Paper', 'Draft')
    expect(store.projects.get('Paper')?.parent_id).toBeNull()
    expect(calls).toHaveLength(0)
    expect(toasts.at(-1)?.kind).toBe('error')
  })

  it('promotes a subproject to the top level', async () => {
    await tree()
    mockApi(() => new Promise(() => {}))
    store.moveProject('Writing', null)
    expect(store.projects.get('Writing')?.parent_id).toBeNull()
    expect(calls[0]).toMatchObject({ method: 'PATCH', path: '/projects/Writing', body: { parent_id: null } })
  })

  it('archives and deletes whole subtrees', async () => {
    await tree()
    mockApi(() => new Promise(() => {}))
    store.updateProject('Writing', { archived: true })
    expect(store.projects.get('Draft')?.archived_at).toBeTruthy()
    expect(store.projects.get('Paper')?.archived_at).toBeNull()
    store.deleteProject('Paper')
    expect([...store.projects.keys()]).toEqual(['Garden'])
    expect(store.tasks.size).toBe(0)
  })
})

describe('project ideas (D-72)', () => {
  it('keeps ideas out of the day, follows the subtree, and unplans when a project becomes one', async () => {
    await load({
      projects: [project('Greenhouse', { status: 'idea' }), project('Garden'), project('Beds', { parent_id: 'Garden' })],
      tasks: [
        task('Compare kits', { project_id: 'Greenhouse', due_date: '2026-10-01' }),
        task('Dig', { project_id: 'Beds' }),
        task('Weed', { project_id: 'Garden' }),
      ],
      day_entries: [entry('E1', 'Dig', '2026-10-06'), entry('E0', 'Weed', '2026-10-05')],
    })
    // Weed (planned on an earlier day) can be pulled in; the idea's task can't.
    expect(store.readyStack('2026-10-06').map((t) => t.id)).toEqual(['Weed'])
    expect(store.inIdea(store.tasks.get('Compare kits')!)).toBe(true)
    expect(store.ideaRoot(store.projects.get('Greenhouse')!)).toBe(true)

    mockApi(() => new Promise(() => {}))
    const sub = store.createProject('Kits', 'Greenhouse')
    expect(store.projects.get(sub)?.status).toBe('idea')
    expect(store.ideaRoot(store.projects.get(sub)!)).toBe(false)
    expect(calls[0]).toMatchObject({ method: 'POST', path: '/projects', body: { status: 'idea' } })

    store.updateProject('Garden', { status: 'idea', description: 'Next spring' })
    expect(store.projects.get('Beds')?.status).toBe('idea')
    expect(store.projects.get('Garden')?.description).toBe('Next spring')
    expect(store.entries.has('E1')).toBe(false)
    expect(store.entries.has('E0')).toBe(true)

    store.updateProject('Greenhouse', { status: 'active' })
    expect(store.readyStack('2026-10-06').map((t) => t.id)).toEqual(['Compare kits'])
  })
})

describe('project marks (D-74)', () => {
  it('gives new projects a colour and shape of their own, and ideas can sit under active projects', async () => {
    await load({ projects: [project('House', { color: '#dc2626', shape: 'circle' })] })
    mockApi(() => new Promise(() => {}))
    const garden = store.createProject('Garden')
    expect(store.projects.get(garden)).toMatchObject({ color: '#1e66cc', shape: 'square' })
    expect(calls[0].body).toMatchObject({ color: '#1e66cc', shape: 'square' })
    const beds = store.createProject('Beds', garden)
    expect(store.projects.get(beds)).toMatchObject({ color: '#1e66cc', shape: 'circle', status: 'active' })
    const pond = store.createProject('Pond', garden, true)
    expect(store.projects.get(pond)).toMatchObject({ shape: 'triangle', status: 'idea' })
    expect(store.ideaRoot(store.projects.get(pond)!)).toBe(true)
  })
})

describe('planning state', () => {
  it('starts a draft, confirms, and keeps a planned day planned when the wizard is reopened', async () => {
    await load()
    const sent: unknown[] = []
    mockApi((_m, _p, body) => {
      sent.push(body)
      return new Promise(() => {})
    })
    store.setPlan('2026-10-07', 'draft', 2)
    const id = store.planFor('2026-10-07')!.id
    expect(store.planFor('2026-10-07')).toMatchObject({ status: 'draft', step: 2 })
    expect(calls[0]).toMatchObject({ method: 'PUT', path: '/days/2026-10-07/plan', body: { id, status: 'draft', step: 2 } })
    store.setPlan('2026-10-07', 'planned', 4)
    expect(store.isPlanned('2026-10-07')).toBe(true)
    expect(store.planFor('2026-10-07')?.planned_at).toBeTruthy()
    store.setPlan('2026-10-07', 'draft', 1)
    expect(store.planFor('2026-10-07')).toMatchObject({ id, status: 'planned', step: 1 })
    store.clearPlan('2026-10-07')
    expect(store.planFor('2026-10-07')).toBeUndefined()
  })

  it('passes server notifications to the app', async () => {
    await load()
    const got: unknown[] = []
    store.onNotification = (n) => got.push(n)
    const es = { listeners: {} as Record<string, (e: MessageEvent) => void>, addEventListener(t: string, f: (e: MessageEvent) => void) { this.listeners[t] = f }, close() {} }
    vi.stubGlobal('EventSource', function () { return es })
    ;(store as unknown as { connect: () => void }).connect()
    es.listeners.change({ data: JSON.stringify({ kind: 'notification', data: { kind: 'plan_evening', title: 'Time to plan tomorrow', body: '', url: '/plan/2026-10-07' } }) } as MessageEvent)
    expect(got).toEqual([{ kind: 'plan_evening', title: 'Time to plan tomorrow', body: '', url: '/plan/2026-10-07' }])
  })
})

describe('routine versions', () => {
  it('counts window progress across a schedule change', async () => {
    const series = (id: string, split_from: string | null) => ({
      id, owner_user_id: 'U1', owner_group_id: null, project_id: null, title: 'Laundry', notes: '', mode: 'flexible' as const,
      rrule: null, dtstart: '2026-10-05', until: null, start_time: null, duration_min: null, times_per_window: 2,
      window: 'week' as const, task_type_id: 'tt_window', estimate_min: null, difficulty: null, importance: null, urgency: null,
      split_from, place_id: null, workflow_template_id: null, workflow_variant_ids: [], checklist: [], created_at: TS, updated_at: TS, deleted_at: null, rev: 1,
    })
    const slot = (id: string, sid: string, status: Task['status']) =>
      task(id, { series_id: sid, occurrence_key: `2026-10-05#${id}`, occurrence_date: '2026-10-05', window_end: '2026-10-11', status })
    await load({
      series: [series('OLD', null), series('NEW', 'OLD')],
      tasks: [slot('a', 'OLD', 'done'), slot('b', 'NEW', 'open'), slot('c', 'NEW', 'open')],
    })
    expect(store.windowProgress('NEW', '2026-10-07')).toEqual({ done: 1, total: 3 })
    expect(store.windowProgress('OLD', '2026-10-07')).toEqual({ done: 1, total: 3 })
  })
})

describe('prerequisites', () => {
  it('completing a prerequisite unblocks its dependents at once (with their wait time)', async () => {
    await load({
      tasks: [
        task('wash'),
        task('dry', { depends_on: ['wash'], blocked: true, wait_min: 60 }),
        task('fold', { depends_on: ['dry'], blocked: true }),
      ],
    })
    mockApi(() => new Promise(() => {}))
    expect(store.readyStack('2026-10-06').map((t) => t.id)).toEqual(['wash'])
    store.updateTask('wash', { status: 'done' })
    const dry = store.tasks.get('dry')!
    expect(dry.blocked).toBe(false)
    expect(dry.ready_at! > new Date().toISOString()).toBe(true)
    expect(store.isReady(dry)).toBe(false)
    expect(store.tasks.get('fold')!.blocked).toBe(true)
    store.updateTask('wash', { status: 'open' })
    expect(store.tasks.get('dry')!.blocked).toBe(true)
  })
})

describe('calendar events and work (D-68)', () => {
  const ev = (id: string, start_at: string, extra: Partial<CalendarEvent> = {}): CalendarEvent => ({
    id, user_id: 'U1', calendar_id: 'C1', uid: 'meeting', instance_key: start_at, title: 'Team meeting', location: null, all_day: false,
    start_at, end_at: start_at.replace('T09', 'T10'), start_date: null, end_date: null, busy: true, recurring: true,
    updated_at: TS, deleted_at: null, rev: 1, ...extra,
  })
  const project: Project = {
    id: 'P1', owner_user_id: 'U1', owner_group_id: null, parent_id: null, name: 'Team', color: null, shape: 'circle', position: 'V',
    archived_at: null, default_place_id: null, description: '', status: 'active', created_at: TS, updated_at: TS, deleted_at: null, rev: 1,
  }

  it('assigns every instance to a project, lists upcoming ones, and attaches todos', async () => {
    await load({
      projects: [project],
      events: [ev('E0', '2026-10-05T09:00:00Z'), ev('E1', '2026-10-07T09:00:00Z'), ev('E2', '2026-10-14T09:00:00Z'), ev('X', '2026-10-08T09:00:00Z', { uid: 'other' })],
    })
    let put: unknown = null
    mockApi((method, path, body) => {
      if (method === 'PUT' && path === '/calendar/event-projects') {
        put = body
        return { ...(body as object), user_id: 'U1', created_at: TS, updated_at: TS, deleted_at: null, rev: 20 }
      }
      return new Promise(() => {})
    })
    await store.setEventProject(store.events.get('E1')!, 'P1')
    expect(put).toMatchObject({ calendar_id: 'C1', uid: 'meeting', project_id: 'P1' })
    expect(store.eventProject(store.events.get('E2')!)?.name).toBe('Team')
    expect(store.eventProject(store.events.get('X')!)).toBeUndefined()
    expect(store.projectEvents('P1').map((e) => e.id)).toEqual(['E1', 'E2'])

    store.createTask({ title: 'Prepare agenda', project_id: 'P1', event_id: 'E1', due_date: store.eventDate(store.events.get('E1')!) })
    expect(store.eventTasks('E1').map((t) => [t.title, t.due_date])).toEqual([['Prepare agenda', '2026-10-07']])
    expect(store.eventTasks('E2')).toEqual([])
  })
})

describe('checklists and waiting (D-70, D-71)', () => {
  it('offers to complete the task when the last step is ticked', async () => {
    const items = [
      { id: 'a', text: 'Sort', done: true },
      { id: 'b', text: 'Wash', done: false },
    ]
    await load({ tasks: [task('T1', { checklist: items })] })
    mockApi((_m, _p, body) => task('T1', { checklist: (body as { checklist: typeof items }).checklist }))
    store.tickItem('T1', 'b')
    expect(store.tasks.get('T1')!.checklist.every((i) => i.done)).toBe(true)
    expect(toasts.at(-1)).toMatchObject({ text: 'All steps done', action: { label: 'Complete task' } })
    await tick()
    expect(calls[0]).toMatchObject({ method: 'PATCH', path: '/tasks/T1', body: { checklist: [{ id: 'a', done: true }, { id: 'b', done: true }] } })
  })

  it('keeps waiting tasks out of the ready stack and lists them by check-back time', async () => {
    await load({
      tasks: [
        task('A'),
        task('B', { waiting_since: TS, check_back_at: '2026-10-06T15:00:00.000Z' }),
        task('C', { waiting_since: TS }),
        task('D', { waiting_since: TS, check_back_at: '2026-10-06T12:00:00.000Z' }),
        task('E', { check_back_at: '2026-10-06T09:00:00.000Z' }),
      ],
    })
    expect(store.readyStack('2026-10-06').map((t) => t.id)).toEqual(['A', 'E'])
    expect(store.waitingTasks().map((t) => t.id)).toEqual(['D', 'B', 'C'])
    expect(store.checkBacks().map((t) => t.id)).toEqual(['E'])
    mockApi(() => task('A', { waiting_since: TS }))
    store.waitFor('A', null, 'reply')
    expect(store.tasks.get('A')).toMatchObject({ waiting_note: 'reply', check_back_at: null })
    expect(store.tasks.get('A')!.started_at).not.toBeNull()
    await tick()
    expect(calls[0].body).toEqual({ waiting: true, check_back_at: null, waiting_note: 'reply' })
  })
})

describe('default tasks (D-71)', () => {
  const tpl = (id: string, title: string, extra = {}) => ({
    id, owner_user_id: 'U1', owner_group_id: null, title, notes: '', checklist: [{ id: 'x', text: 'Sort', done: false }],
    estimate_min: 20, difficulty: null, importance: null, urgency: null, task_type_id: null, project_id: null, place_id: null,
    created_at: TS, updated_at: TS, deleted_at: null, rev: 1, ...extra,
  })

  it('suggests matches and creates tasks with a fresh checklist', async () => {
    await load({ projects: [project('P1')], task_templates: [tpl('D1', 'Do laundry'), tpl('D2', 'Water plants'), tpl('D3', 'Laundry towels')] })
    expect(store.matchDefaults('laun').map((t) => t.id)).toEqual(['D3', 'D1'])
    expect(store.matchDefaults('l')).toEqual([])
    mockApi((_m, _p, body) => task((body as { id: string }).id, body as Partial<Task>))
    const id = store.createFromDefault('D1', { day: '2026-10-06', project_id: 'P1' })!
    const t = store.tasks.get(id)!
    expect(t).toMatchObject({ title: 'Do laundry', estimate_min: 20, project_id: 'P1' })
    expect(t.checklist).toHaveLength(1)
    expect(t.checklist[0].id).not.toBe('x')
    expect(store.entryForTask(id)?.date).toBe('2026-10-06')
  })
})

describe('health check-in (D-75)', () => {
  it('keeps one answer per day', async () => {
    await load()
    mockApi((_m, path, body) => ({ id: 'H1', user_id: 'U1', date: path.split('/')[2], note: '', kind: null, created_at: TS, updated_at: TS, deleted_at: null, rev: 2, ...(body as object) }))
    store.setHealth('2026-10-06', 'sick', 'flu')
    await tick()
    store.setHealth('2026-10-06', 'unwell')
    await tick()
    expect([...store.healthDays.values()]).toHaveLength(1)
    expect(store.healthOn('2026-10-06')).toMatchObject({ status: 'unwell', kind: null })
    expect(calls.map((c) => [c.method, c.path])).toEqual([
      ['PUT', '/days/2026-10-06/health'],
      ['PUT', '/days/2026-10-06/health'],
    ])
  })
})
