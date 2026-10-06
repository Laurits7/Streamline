<script lang="ts">
  import { api, ApiError, onUnauthorized } from './lib/api/client'
  import Icon from './lib/components/Icon.svelte'
  import PullSheet from './lib/components/PullSheet.svelte'
  import TaskSheet from './lib/components/TaskSheet.svelte'
  import Toasts from './lib/components/Toasts.svelte'
  import FocusTicker from './lib/components/FocusTicker.svelte'
  import MiniTimer from './lib/components/MiniTimer.svelte'
  import { announce, live } from './lib/announce.svelte'
  import AllTasks from './views/AllTasks.svelte'
  import Routines from './views/Routines.svelte'
  import RoutineSheet from './lib/components/RoutineSheet.svelte'
  import PlaceTracker from './lib/components/PlaceTracker.svelte'
  import PlaceSwitcher from './lib/components/PlaceSwitcher.svelte'
  import { droppable, type DragItem } from './lib/dnd.svelte'
  import { match, router } from './lib/router.svelte'
  import { store } from './lib/store.svelte'
  import { isCollapsed, toggleCollapsed, ui } from './lib/ui.svelte'
  import Day from './views/Day.svelte'
  import Login from './views/Login.svelte'
  import Projects from './views/Projects.svelte'
  import Settings from './views/Settings.svelte'
  import TaskList from './views/TaskList.svelte'
  import { toast } from './lib/toast.svelte'

  let phase = $state<'loading' | 'login' | 'setup' | 'app' | 'error'>('loading')

  async function boot() {
    phase = 'loading'
    try {
      await store.start()
      phase = 'app'
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) {
        try {
          const s = await api.get<{ needs_setup: boolean }>('/setup')
          phase = s.needs_setup ? 'setup' : 'login'
        } catch {
          phase = 'error'
        }
      } else {
        phase = 'error'
      }
    }
  }

  async function logout() {
    try {
      await api.post('/auth/logout')
    } catch {
      /* ignore */
    }
    store.stop()
    ui.editing = null
    ui.pullFor = null
    phase = 'login'
  }

  // Reminders and other notifications pushed by the server.
  store.onNotification = (n) => {
    // Focus notifications are already signalled by this device's own timer (FocusTicker).
    if (n.kind === 'focus') return
    toast(n.title, 'info', { label: 'Plan now', run: () => router.go(n.url) })
  }

  $effect(() => {
    boot()
    return onUnauthorized(() => {
      if (phase === 'app') {
        store.stop()
        phase = 'login'
      }
    })
  })

  // Let the browser know the page language (affects hyphenation, screen readers).
  $effect(() => {
    document.documentElement.lang = store.me?.locale || navigator.language || 'en'
  })

  const route = $derived(match(router.path))
  const projects = $derived(store.projectTree(isCollapsed))
  const active = (name: string, id?: string) =>
    route.name === name && (!id || (route.name === 'project' && route.id === id)) ? 'page' : undefined
  const todayActive = $derived(route.name === 'today' || route.name === 'day' ? 'page' : undefined)
  // Board and matrix views need room: widen the page when one is shown.
  const viewKey = $derived(
    route.name === 'tasks' ? 'view:all' : route.name === 'inbox' ? 'view:inbox' : route.name === 'project' ? `view:project:${route.id}` : null,
  )
  const wideView = $derived(!!viewKey && store.pref<{ view?: string }>(viewKey, {}).view !== undefined && store.pref<{ view?: string }>(viewKey, {}).view !== 'list')
  const isDayRoute = $derived(route.name === 'today' || route.name === 'day' || route.name === 'plan' || wideView)

  // Navigation links double as drop targets for tasks.
  const toProject = (projectId: string | null) => ({
    accepts: (it: DragItem) => it.kind === 'task',
    drop: (it: DragItem) => {
      if (it.kind !== 'task') return
      store.moveToProject(it.taskId, projectId)
      announce(`Moved “${store.tasks.get(it.taskId)?.title}” to ${projectId ? store.projects.get(projectId)?.name : 'Inbox'}`)
    },
  })
  const toToday = {
    accepts: (it: DragItem) => it.kind === 'task',
    drop: (it: DragItem) => {
      if (it.kind !== 'task') return
      store.plan(it.taskId, store.today)
      announce(`Planned “${store.tasks.get(it.taskId)?.title}” for today`)
    },
  }
  const projectsActive = $derived(route.name === 'projects' || route.name === 'project' ? 'page' : undefined)
</script>

{#if phase === 'loading'}
  <div class="splash" aria-busy="true"></div>
{:else if phase === 'error'}
  <div class="splash">
    <p>Can't reach the server.</p>
    <button class="btn" onclick={boot}>Retry</button>
  </div>
{:else if phase === 'login' || phase === 'setup'}
  <Login setup={phase === 'setup'} onlogin={boot} />
{:else if route.name === 'focus'}
  <FocusTicker />
  {#await import('./views/Focus.svelte') then m}<m.default />{/await}
  {#if ui.editing}<TaskSheet id={ui.editing} />{/if}
{:else}
  <FocusTicker />
  <MiniTimer />
  <PlaceTracker />
  <div class="shell">
    <aside class="sidebar">
      <div class="brand">
        <svg viewBox="0 0 64 64" width="26" height="26" aria-hidden="true"><rect width="64" height="64" rx="14" fill="var(--accent)" /><path d="M18 34l9 9 19-21" fill="none" stroke="var(--accent-text)" stroke-width="6" stroke-linecap="round" stroke-linejoin="round" /></svg>
        Streamline
        <span class="live" class:on={store.live} title={store.live ? 'Live sync connected' : 'Reconnecting…'}></span>
      </div>
      <div class="place"><PlaceSwitcher /></div>
      <nav aria-label="Main">
        <a href="/" class="drop-zone" draggable="false" aria-current={todayActive} use:droppable={toToday}><Icon name="sun" /> Today</a>
        <a href="/inbox" class="drop-zone" draggable="false" aria-current={active('inbox')} use:droppable={toProject(null)}><Icon name="inbox" /> Inbox</a>
        <a href="/tasks" draggable="false" aria-current={active('tasks')}><Icon name="list" /> All tasks</a>
        <a href="/routines" draggable="false" aria-current={active('routines')}><Icon name="repeat" /> Routines</a>
        <a href="/focus" draggable="false"><Icon name="target" /> Focus</a>
        <a href="/projects" draggable="false" aria-current={active('projects')}><Icon name="folder" /> Projects</a>
        <div class="projects">
          {#each projects as { project: p, depth } (p.id)}
            {@const kids = store.childProjects(p.id).length > 0}
            <div class="tree-row" style:padding-left="{depth * 14}px">
              {#if kids}
                <button
                  class="twisty"
                  aria-label="{isCollapsed(p.id) ? 'Expand' : 'Collapse'} {p.name}"
                  aria-expanded={!isCollapsed(p.id)}
                  onclick={() => toggleCollapsed(p.id)}><span class:open={!isCollapsed(p.id)}><Icon name="right" size={12} /></span></button>
              {:else}<span class="twisty"></span>{/if}
              <a
                href="/projects/{p.id}"
                class="drop-zone"
                draggable="false"
                aria-current={active('project', p.id)}
                use:droppable={toProject(p.id)}><i style:background={p.color ?? 'var(--faint)'}></i>{p.name}</a>
            </div>
          {/each}
        </div>
      </nav>
      <a class="settings" href="/help" aria-current={active('help')}><Icon name="help" /> Help</a>
      <a href="/settings" aria-current={active('settings')}><Icon name="settings" /> {store.me?.display_name}</a>
    </aside>

    <main class="content" class:wide={isDayRoute}>
      {#if route.name === 'today'}
        <Day />
      {:else if route.name === 'day'}
        <Day date={route.date} />
      {:else if route.name === 'plan'}
        {#await import('./views/Plan.svelte') then m}{#key route.date}<m.default date={route.date} />{/key}{/await}
      {:else if route.name === 'routines'}
        <Routines />
      {:else if route.name === 'tasks'}
        <AllTasks />
      {:else if route.name === 'inbox'}
        <TaskList />
      {:else if route.name === 'projects'}
        <Projects />
      {:else if route.name === 'project'}
        {#key route.id}<TaskList projectId={route.id} />{/key}
      {:else if route.name === 'help'}
        {#await import('./views/Help.svelte') then m}<m.default />{/await}
      {:else if route.name === 'settings'}
        <Settings onlogout={logout} />
      {:else}
        <p class="empty">Page not found. <a href="/">Go to today</a></p>
      {/if}
    </main>

    <nav class="tabbar" aria-label="Main">
      <a href="/" class="drop-zone" draggable="false" aria-current={todayActive} use:droppable={toToday}><Icon name="sun" size={22} /><span>Today</span></a>
      <a href="/inbox" class="drop-zone" draggable="false" aria-current={active('inbox')} use:droppable={toProject(null)}><Icon name="inbox" size={22} /><span>Inbox</span></a>
      <a href="/projects" aria-current={projectsActive}><Icon name="folder" size={22} /><span>Projects</span></a>
      <a href="/settings" aria-current={route.name === 'settings' || route.name === 'help' ? 'page' : undefined}><Icon name="settings" size={22} /><span>Settings</span></a>
    </nav>
  </div>

  {#if ui.editing}<TaskSheet id={ui.editing} />{/if}
  {#if ui.pullFor}<PullSheet date={ui.pullFor} />{/if}
  {#if ui.routine}{#key ui.routine}<RoutineSheet id={ui.routine} />{/key}{/if}
{/if}

<Toasts />
<div class="sr-only" aria-live="polite">{live.message}</div>

<style>
  .splash {
    min-height: 100dvh;
    display: grid;
    place-content: center;
    gap: 12px;
    text-align: center;
  }
  .shell {
    min-height: 100dvh;
  }
  .content {
    --content-w: 760px;
    max-width: var(--content-w);
    margin: 0 auto;
    padding: calc(20px + env(safe-area-inset-top)) 16px calc(var(--nav-h) + 32px + env(safe-area-inset-bottom));
  }
  .content.wide {
    --content-w: 1160px;
  }
  .sidebar {
    display: none;
  }
  .tabbar {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 40;
    height: calc(var(--nav-h) + env(safe-area-inset-bottom));
    padding-bottom: env(safe-area-inset-bottom);
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    border-top: 1px solid var(--border);
  }
  .tabbar a {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    font-size: 11px;
    font-weight: 600;
    color: var(--faint);
  }
  .tabbar a[aria-current='page'] {
    color: var(--accent);
  }

  @media (min-width: 900px) {
    .tabbar {
      display: none;
    }
    .sidebar {
      display: flex;
      flex-direction: column;
      position: fixed;
      inset: 0 auto 0 0;
      width: var(--sidebar-w);
      padding: 18px 12px;
      border-right: 1px solid var(--border);
      background: var(--surface);
      overflow-y: auto;
    }
    .content {
      margin-left: calc(var(--sidebar-w) + max(0px, (100vw - var(--sidebar-w) - var(--content-w)) / 2));
      padding: 36px 32px 48px;
    }
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 750;
    font-size: 17px;
    padding: 4px 10px 18px;
  }
  .place {
    padding: 0 6px 12px;
  }
  .live {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--faint);
    margin-left: auto;
  }
  .live.on {
    background: var(--ok);
  }
  .sidebar nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }
  .sidebar a {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 8px;
    color: var(--muted);
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sidebar a:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .sidebar a[aria-current='page'] {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .projects {
    margin: 6px 0 0 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .projects a {
    flex: 1;
    min-width: 0;
    font-size: 14px;
    padding: 6px 8px;
  }
  .tree-row {
    display: flex;
    align-items: center;
  }
  .twisty {
    flex: none;
    width: 16px;
    height: 24px;
    display: grid;
    place-items: center;
    color: var(--faint);
  }
  .twisty span {
    display: grid;
    transition: transform 120ms;
  }
  .twisty span.open {
    transform: rotate(90deg);
  }
  .projects i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }
  .settings {
    margin-top: 12px;
  }
</style>
