// The Linux desktop app (D-78) runs this web app in a window and reaches the server for
// it: requests and live events go through the app (`invoke`, `listen`), which adds the
// device token from the system keyring. In a browser none of this is used.

type Tauri = {
  core: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> }
  event: { listen: <T>(name: string, cb: (e: { payload: T }) => void) => Promise<() => void> }
}

const tauri = (): Tauri | undefined => (globalThis as { __TAURI__?: Tauri }).__TAURI__

/** Running inside the desktop app. */
export const desktop = typeof window !== 'undefined' && !!tauri()

export const invoke = <T>(cmd: string, args?: Record<string, unknown>) => tauri()!.core.invoke<T>(cmd, args)
export const listen = <T>(name: string, cb: (payload: T) => void) => tauri()!.event.listen<T>(name, (e) => cb(e.payload))

export type DesktopStatus = { server: string | null; username: string | null; signed_in: boolean }

/**
 * Live events from the server, relayed by the app: the subset of `EventSource` the store
 * uses (named events, `onerror`, `readyState`, `close`).
 */
export class DesktopEvents {
  readyState = 0
  onerror: (() => void) | null = null
  private handlers = new Map<string, ((e: { data: string }) => void)[]>()
  private unlisten: (() => void)[] = []
  private closed = false

  constructor() {
    const events = listen<{ event: string; data: string }>('server-event', ({ event, data }) => {
      this.readyState = 1
      for (const h of this.handlers.get(event) ?? []) h({ data })
    }).then((u) => this.keep(u))
    const stream = listen<{ open: boolean; unauthorized?: boolean }>('server-stream', ({ open, unauthorized }) => {
      if (this.closed) return
      this.readyState = open ? 1 : unauthorized ? 2 : 0
      if (!open) this.onerror?.()
    }).then((u) => this.keep(u))
    // Listening first, then a fresh stream: its `hello` makes the page catch up.
    Promise.all([events, stream]).then(() => !this.closed && invoke('events_open'))
  }
  private keep(u: () => void) {
    if (this.closed) u()
    else this.unlisten.push(u)
  }
  addEventListener(name: string, h: (e: { data: string }) => void) {
    this.handlers.set(name, [...(this.handlers.get(name) ?? []), h])
  }
  close() {
    if (this.closed) return
    this.closed = true
    this.readyState = 2
    this.unlisten.forEach((u) => u())
    invoke('events_close').catch(() => {})
  }
}
