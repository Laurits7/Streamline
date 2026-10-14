// Thin JSON client for /api/v1. Cookies carry the session (same origin); in the desktop
// app the app makes the request with its device token (D-78).
import { connection } from '../connection.svelte'
import { desktop, invoke } from '../desktop'

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message)
  }
}

type Listener = () => void
const unauthorizedListeners = new Set<Listener>()
export const onUnauthorized = (l: Listener) => {
  unauthorizedListeners.add(l)
  return () => unauthorizedListeners.delete(l)
}

async function viaDesktop<T>(method: string, path: string, body?: unknown): Promise<T> {
  let r: { status: number; body: unknown; offline: boolean }
  try {
    r = await invoke('api', { method, path, body: body ?? null })
  } catch (e) {
    throw new ApiError(400, String(e))
  }
  connection.offline = r.offline || r.status === 0
  if (r.status === 0) throw new ApiError(0, 'You appear to be offline')
  if (r.status === 401 && path !== '/auth/login') unauthorizedListeners.forEach((l) => l())
  if (r.status >= 400) {
    const p = (r.body ?? {}) as { detail?: string; title?: string }
    throw new ApiError(r.status, p.detail ?? p.title ?? `Error ${r.status}`)
  }
  return r.body as T
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  if (desktop) return viaDesktop<T>(method, path, body)
  let res: Response
  try {
    res = await fetch(`/api/v1${path}`, {
      method,
      credentials: 'same-origin',
      headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    })
  } catch {
    connection.offline = true
    throw new ApiError(0, 'You appear to be offline')
  }
  connection.offline = res.headers.get('X-Streamline-Offline') === '1'
  if (res.status === 401 && path !== '/auth/login') unauthorizedListeners.forEach((l) => l())
  if (!res.ok) {
    let msg = res.statusText
    try {
      const p = await res.json()
      msg = p.detail ?? p.title ?? msg
    } catch {
      /* not JSON */
    }
    throw new ApiError(res.status, msg)
  }
  if (res.status === 204) return undefined as T
  return res.json() as Promise<T>
}

export const api = {
  get: <T>(p: string) => request<T>('GET', p),
  post: <T>(p: string, b?: unknown) => request<T>('POST', p, b ?? {}),
  patch: <T>(p: string, b: unknown) => request<T>('PATCH', p, b),
  put: <T>(p: string, b: unknown) => request<T>('PUT', p, b),
  del: <T = void>(p: string) => request<T>('DELETE', p),
}
