// Tiny history-API router. Routes are matched in App.svelte.

class Router {
  path = $state(location.pathname)

  go(path: string, replace = false) {
    if (path === this.path) return
    if (replace) history.replaceState({}, '', path)
    else history.pushState({}, '', path)
    this.path = path
    window.scrollTo(0, 0)
  }
}

export const router = new Router()

window.addEventListener('popstate', () => (router.path = location.pathname))

// Intercept same-origin <a href="/..."> clicks so navigation stays client-side.
document.addEventListener('click', (e) => {
  if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return
  const a = (e.target as Element).closest?.('a')
  if (!a || a.target || a.hasAttribute('download')) return
  const href = a.getAttribute('href')
  if (!href || !href.startsWith('/') || href.startsWith('//') || href.startsWith('/api/')) return
  e.preventDefault()
  router.go(href)
})

export type Route =
  | { name: 'today' }
  | { name: 'day'; date: string }
  | { name: 'plan'; date: string }
  | { name: 'inbox' }
  | { name: 'projects' }
  | { name: 'project'; id: string }
  | { name: 'settings' }
  | { name: 'help' }
  | { name: 'notfound' }

export function match(path: string): Route {
  const p = path.replace(/\/+$/, '') || '/'
  if (p === '/') return { name: 'today' }
  let m
  if ((m = p.match(/^\/day\/(\d{4}-\d{2}-\d{2})$/))) return { name: 'day', date: m[1] }
  if ((m = p.match(/^\/plan\/(\d{4}-\d{2}-\d{2})$/))) return { name: 'plan', date: m[1] }
  if (p === '/inbox') return { name: 'inbox' }
  if (p === '/projects') return { name: 'projects' }
  if ((m = p.match(/^\/projects\/([0-9A-Za-z]+)$/))) return { name: 'project', id: m[1] }
  if (p === '/settings') return { name: 'settings' }
  if (p === '/help') return { name: 'help' }
  return { name: 'notfound' }
}
