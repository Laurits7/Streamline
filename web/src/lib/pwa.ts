// Installable web app (Phase 6): the service worker and Web Push. Everything here only
// works in a secure context (HTTPS or localhost); over plain HTTP the app runs as before.
import { api } from './api/client'
import { router } from './router.svelte'

export const secure = typeof window !== 'undefined' && window.isSecureContext && 'serviceWorker' in navigator

export function registerServiceWorker() {
  if (!secure) return
  navigator.serviceWorker.register('/sw.js').catch(() => {})
  // A tapped notification asks an open window to show its page.
  navigator.serviceWorker.addEventListener('message', (e) => {
    if (e.data?.type === 'open' && typeof e.data.url === 'string') router.go(e.data.url)
  })
}

/** Forget cached data on this device (after signing out). */
export function clearOfflineData() {
  if (secure) navigator.serviceWorker.controller?.postMessage('clear-data')
}

/** Installed to the Home Screen (iOS) or as an app (others). */
export const standalone = () =>
  window.matchMedia?.('(display-mode: standalone)').matches || (navigator as { standalone?: boolean }).standalone === true

export const isIOS = () => /iPad|iPhone|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1)

/** Why push can't be used on this device, or null if it can. */
export function pushBlocker(): string | null {
  if (!window.isSecureContext) return 'Notifications need Streamline to be opened over HTTPS (see the README).'
  if (isIOS() && !standalone()) return 'On iPhone and iPad, first add Streamline to the Home Screen (Share → Add to Home Screen) and open it from there.'
  if (!('serviceWorker' in navigator) || !('PushManager' in window)) return "This browser doesn't support push notifications."
  if (Notification.permission === 'denied') return 'Notifications are blocked for Streamline in this browser’s settings.'
  return null
}

const b64ToBytes = (s: string) => {
  const b = atob(s.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (s.length % 4)) % 4))
  return Uint8Array.from(b, (c) => c.charCodeAt(0))
}

async function registration() {
  return navigator.serviceWorker.ready
}

export async function pushEnabled(): Promise<boolean> {
  if (pushBlocker()) return false
  const sub = await (await registration()).pushManager.getSubscription()
  return !!sub && Notification.permission === 'granted'
}

/** Ask for permission and register this device (must run from a tap). */
export async function enablePush(): Promise<void> {
  const blocker = pushBlocker()
  if (blocker) throw new Error(blocker)
  if ((await Notification.requestPermission()) !== 'granted') throw new Error('Notifications were not allowed.')
  const { public_key } = await api.get<{ public_key: string }>('/push/key')
  const reg = await registration()
  const sub =
    (await reg.pushManager.getSubscription()) ??
    (await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: b64ToBytes(public_key) }))
  const json = sub.toJSON() as { endpoint: string; keys: { p256dh: string; auth: string } }
  await api.post('/push/subscriptions', { endpoint: json.endpoint, keys: json.keys })
}

export async function disablePush(): Promise<void> {
  const sub = await (await registration()).pushManager.getSubscription()
  if (!sub) return
  await api.post('/push/subscriptions/remove', { endpoint: sub.endpoint }).catch(() => {})
  await sub.unsubscribe()
}
