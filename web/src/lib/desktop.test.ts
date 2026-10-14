import { afterEach, describe, expect, it, vi } from 'vitest'

// A fake of the app's bridge: `invoke` answers like the app's `api` command, `listen`
// lets the test emit the app's events.
function fakeTauri(answer: (cmd: string, args: Record<string, unknown>) => unknown) {
  const listeners = new Map<string, ((e: { payload: unknown }) => void)[]>()
  ;(globalThis as Record<string, unknown>).__TAURI__ = {
    core: { invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => answer(cmd, args)) },
    event: {
      listen: async (name: string, cb: (e: { payload: unknown }) => void) => {
        listeners.set(name, [...(listeners.get(name) ?? []), cb])
        return () => listeners.set(name, (listeners.get(name) ?? []).filter((x) => x !== cb))
      },
    },
  }
  return (name: string, payload: unknown) => listeners.get(name)?.forEach((cb) => cb({ payload }))
}

afterEach(() => {
  delete (globalThis as Record<string, unknown>).__TAURI__
  vi.resetModules()
})

describe('desktop app bridge (D-78)', () => {
  it('sends API calls through the app and maps its answers', async () => {
    fakeTauri((_cmd, args) => {
      if (args.path === '/me') return { status: 200, body: { id: 'U1' }, offline: false }
      if (args.path === '/gone') return { status: 404, body: { title: 'Not Found' }, offline: false }
      return { status: 0, body: null, offline: true }
    })
    const { api, ApiError } = await import('./api/client')
    const { connection } = await import('./connection.svelte')
    expect(await api.get('/me')).toEqual({ id: 'U1' })
    expect(connection.offline).toBe(false)
    await expect(api.patch('/gone', { title: 'x' })).rejects.toMatchObject({ status: 404, message: 'Not Found' })
    await expect(api.get('/far')).rejects.toBeInstanceOf(ApiError)
    expect(connection.offline).toBe(true)
  })

  it('tells the app which records a write changed, so it can wait offline (9.5)', async () => {
    const calls: Record<string, unknown>[] = []
    fakeTauri((_cmd, args) => {
      calls.push(args)
      return { status: 200, body: { id: 'T1', title: 'Laundry (offline)' }, offline: true }
    })
    const { api, withTouched } = await import('./api/client')
    const t = await withTouched([['task', 'T1', { id: 'T1', title: 'Laundry (offline)' }]], () => api.patch('/tasks/T1', { title: 'Laundry (offline)' }))
    expect(t).toEqual({ id: 'T1', title: 'Laundry (offline)' })
    await api.get('/sync?since=0')
    expect(calls.map((c) => c.touched)).toEqual([[['task', 'T1', { id: 'T1', title: 'Laundry (offline)' }]], null])
  })

  it('relays live events like an EventSource', async () => {
    const emit = fakeTauri(() => null)
    const { DesktopEvents } = await import('./desktop')
    const es = new DesktopEvents()
    await new Promise((r) => setTimeout(r))
    const invoke = (globalThis as { __TAURI__?: { core: { invoke: { mock: { calls: unknown[][] } } } } }).__TAURI__!.core.invoke
    // The stream is opened once the page listens.
    expect(invoke.mock.calls.map((c) => c[0])).toEqual(['events_open'])
    const got: string[] = []
    let errors = 0
    es.addEventListener('change', (e) => got.push(e.data))
    es.onerror = () => errors++
    emit('server-stream', { open: true })
    emit('server-event', { event: 'change', data: '{"kind":"task"}' })
    emit('server-event', { event: 'hello', data: '' })
    expect(got).toEqual(['{"kind":"task"}'])
    expect(es.readyState).toBe(1)
    emit('server-stream', { open: false, unauthorized: true })
    expect([errors, es.readyState]).toEqual([1, 2])
    es.close()
    emit('server-event', { event: 'change', data: 'late' })
    expect(got).toHaveLength(1)
    expect(invoke.mock.calls.map((c) => c[0])).toEqual(['events_open', 'events_close'])
  })
})
