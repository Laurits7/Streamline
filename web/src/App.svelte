<script lang="ts">
  import { api, ApiError, onUnauthorized } from './lib/api/client'
  import Icon from './lib/components/Icon.svelte'
  import PullSheet from './lib/components/PullSheet.svelte'
  import TaskSheet from './lib/components/TaskSheet.svelte'
  import Toasts from './lib/components/Toasts.svelte'
  import { match, router } from './lib/router.svelte'
  import { store } from './lib/store.svelte'
  import { ui } from './lib/ui.svelte'
  import Day from './views/Day.svelte'
  import Login from './views/Login.svelte'
  import Projects from './views/Projects.svelte'
  import Settings from './views/Settings.svelte'
  import TaskList from './views/TaskList.svelte'

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

  $effect(() => {
    boot()
    return onUnauthorized(() => {
      if (phase === 'app') {
        store.stop()
        phase = 'login'
      }
    })
  })

  const route = $derived(match(router.path))
  const projects = $derived(store.projectList())
  const active = (name: string, id?: string) =>
    route.name === name && (!id || (route.name === 'project' && route.id === id)) ? 'page' : undefined
  const todayActive = $derived(route.name === 'today' || route.name === 'day' ? 'page' : undefined)
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
{:else}
  <div class="shell">
    <aside class="sidebar">
      <div class="brand">
        <svg viewBox="0 0 64 64" width="26" height="26" aria-hidden="true"><rect width="64" height="64" rx="14" fill="var(--accent)" /><path d="M18 34l9 9 19-21" fill="none" stroke="var(--accent-text)" stroke-width="6" stroke-linecap="round" stroke-linejoin="round" /></svg>
        Streamline
        <span class="live" class:on={store.live} title={store.live ? 'Live sync connected' : 'Reconnecting…'}></span>
      </div>
      <nav aria-label="Main">
        <a href="/" aria-current={todayActive}><Icon name="sun" /> Today</a>
        <a href="/inbox" aria-current={active('inbox')}><Icon name="inbox" /> Inbox</a>
        <a href="/projects" aria-current={active('projects')}><Icon name="folder" /> Projects</a>
        <div class="projects">
          {#each projects as p (p.id)}
            <a href="/projects/{p.id}" aria-current={active('project', p.id)}><i style:background={p.color ?? 'var(--faint)'}></i>{p.name}</a>
          {/each}
        </div>
      </nav>
      <a class="settings" href="/settings" aria-current={active('settings')}><Icon name="settings" /> {store.me?.display_name}</a>
    </aside>

    <main class="content">
      {#if route.name === 'today'}
        <Day />
      {:else if route.name === 'day'}
        <Day date={route.date} />
      {:else if route.name === 'inbox'}
        <TaskList />
      {:else if route.name === 'projects'}
        <Projects />
      {:else if route.name === 'project'}
        {#key route.id}<TaskList projectId={route.id} />{/key}
      {:else if route.name === 'settings'}
        <Settings onlogout={logout} />
      {:else}
        <p class="empty">Page not found. <a href="/">Go to today</a></p>
      {/if}
    </main>

    <nav class="tabbar" aria-label="Main">
      <a href="/" aria-current={todayActive}><Icon name="sun" size={22} /><span>Today</span></a>
      <a href="/inbox" aria-current={active('inbox')}><Icon name="inbox" size={22} /><span>Inbox</span></a>
      <a href="/projects" aria-current={projectsActive}><Icon name="folder" size={22} /><span>Projects</span></a>
      <a href="/settings" aria-current={active('settings')}><Icon name="settings" size={22} /><span>Settings</span></a>
    </nav>
  </div>

  {#if ui.editing}<TaskSheet id={ui.editing} />{/if}
  {#if ui.pullFor}<PullSheet date={ui.pullFor} />{/if}
{/if}

<Toasts />

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
    max-width: 760px;
    margin: 0 auto;
    padding: calc(20px + env(safe-area-inset-top)) 16px calc(var(--nav-h) + 32px + env(safe-area-inset-bottom));
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
      margin-left: calc(var(--sidebar-w) + max(0px, (100vw - var(--sidebar-w) - 760px) / 2));
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
    margin: 6px 0 0 8px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .projects a {
    font-size: 14px;
    padding: 6px 10px;
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
