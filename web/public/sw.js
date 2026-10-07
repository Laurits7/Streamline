// Streamline service worker (SPEC §7.6, Phase 6). Registered only over HTTPS.
//  - App shell: pages are fetched fresh, falling back to the cached shell when offline;
//    content-hashed assets are cached on first use.
//  - Data: GET API reads are fetched fresh and the last answer is kept, so the app opens
//    read-only without a connection (the response is marked `X-Streamline-Offline`).
//  - Push: shows notifications sent by the server, and opens the right page on tap.
// Bump SHELL when files in /icons change: they are served cache-first and aren't content-hashed.
const SHELL = 'sl-shell-v2'
const DATA = 'sl-data-v1'
// Reads worth keeping for offline use (not the live event stream).
const KEEP = [/^\/api\/v1\/(sync|me|today|namedays|setup)(\?|$)/, /^\/api\/v1\/days\/[^/]+(\/summary)?$/]

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(SHELL)
      .then((c) => c.addAll(['/', '/manifest.webmanifest', '/icons/icon-192.png', '/icons/icon.svg']))
      .then(() => self.skipWaiting()),
  )
})

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => ![SHELL, DATA].includes(k)).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  )
})

async function shell(request) {
  try {
    const res = await fetch(request)
    if (res.ok) (await caches.open(SHELL)).put('/', res.clone())
    return res
  } catch {
    return (await caches.match('/')) ?? Response.error()
  }
}

async function asset(request) {
  const hit = await caches.match(request)
  if (hit) return hit
  const res = await fetch(request)
  if (res.ok) (await caches.open(SHELL)).put(request, res.clone())
  return res
}

async function data(request) {
  const cache = await caches.open(DATA)
  try {
    const res = await fetch(request)
    if (res.ok) cache.put(request, res.clone())
    return res
  } catch {
    // A delta sync falls back to the last full one.
    const url = new URL(request.url)
    const key = url.pathname === '/api/v1/sync' ? new Request(url.origin + '/api/v1/sync?since=0') : request
    const hit = (await cache.match(request)) ?? (await cache.match(key))
    if (!hit) return Response.error()
    const headers = new Headers(hit.headers)
    headers.set('X-Streamline-Offline', '1')
    return new Response(await hit.blob(), { status: hit.status, headers })
  }
}

self.addEventListener('fetch', (event) => {
  const req = event.request
  if (req.method !== 'GET') return
  const url = new URL(req.url)
  if (url.origin !== self.location.origin) return
  if (req.mode === 'navigate') return event.respondWith(shell(req))
  if (url.pathname.startsWith('/assets/') || url.pathname.startsWith('/icons/')) return event.respondWith(asset(req))
  if (KEEP.some((r) => r.test(url.pathname + url.search))) return event.respondWith(data(req))
})

self.addEventListener('message', (event) => {
  // Signed out: forget the cached data.
  if (event.data === 'clear-data') event.waitUntil(caches.delete(DATA))
})

self.addEventListener('push', (event) => {
  let n = { title: 'Streamline', body: '', url: '/', kind: 'info' }
  try {
    n = { ...n, ...event.data.json() }
  } catch {
    if (event.data) n.body = event.data.text()
  }
  event.waitUntil(
    self.registration.showNotification(n.title, {
      body: n.body,
      tag: n.kind,
      icon: '/icons/icon-192.png',
      badge: '/icons/icon-192.png',
      data: { url: n.url },
    }),
  )
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const url = event.notification.data?.url ?? '/'
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((wins) => {
      const win = wins.find((w) => new URL(w.url).origin === self.location.origin)
      if (win) {
        win.postMessage({ type: 'open', url })
        return win.focus()
      }
      return self.clients.openWindow(url)
    }),
  )
})
