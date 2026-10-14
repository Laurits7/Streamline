<script lang="ts">
  import { api, ApiError } from '../lib/api/client'
  import { desktop, invoke, type DesktopStatus } from '../lib/desktop'

  let { setup, onlogin }: { setup: boolean; onlogin: () => void } = $props()
  let username = $state('')
  let password = $state('')
  let displayName = $state('')
  let error = $state('')
  let busy = $state(false)
  // Desktop app (D-78): connect to a server (remembered from last time), or run on this
  // computer only (D-88).
  let server = $state('')
  let choice = $state<'server' | 'local'>('server')
  if (desktop)
    invoke<DesktopStatus>('status').then((s) => {
      server ||= s.server ?? ''
      username ||= s.username ?? ''
    })

  async function submit(e: Event) {
    e.preventDefault()
    busy = true
    error = ''
    if (desktop) {
      try {
        if (choice === 'local') await invoke('use_local', { name: displayName })
        else await invoke('connect', { server, username, password })
        password = ''
        onlogin()
      } catch (e) {
        error = String(e)
      } finally {
        busy = false
      }
      return
    }
    try {
      if (setup) await api.post('/setup', { username, password, display_name: displayName || null })
      else await api.post('/auth/login', { username, password })
      onlogin()
    } catch (e) {
      error =
        e instanceof ApiError
          ? e.status === 401
            ? 'Wrong username or password.'
            : e.message
          : 'Could not reach the server.'
    } finally {
      busy = false
    }
  }
</script>

<main>
  <form class="card" onsubmit={submit}>
    <div class="logo" aria-hidden="true">
      <img src="/icons/icon.svg" width="44" height="44" alt="" />
    </div>
    <h1>{setup ? 'Welcome to Streamline' : desktop ? 'Welcome to Streamline' : 'Sign in'}</h1>
    {#if setup}<p class="muted">Create the admin account for this household. You can add more people later in Settings.</p>{/if}
    {#if desktop}
      <div class="choice" role="radiogroup" aria-label="How to use Streamline">
        <button type="button" role="radio" aria-checked={choice === 'server'} class:on={choice === 'server'} onclick={() => ((choice = 'server'), (error = ''))}>Connect to a server</button>
        <button type="button" role="radio" aria-checked={choice === 'local'} class:on={choice === 'local'} onclick={() => ((choice = 'local'), (error = ''))}>This computer only</button>
      </div>
    {/if}
    {#if desktop && choice === 'local'}
      <p class="muted">
        Streamline runs inside this app and keeps your data on this computer. It works without a network, but only here:
        to use it on your phone too, run a Streamline server later and move your data to it (see Help).
      </p>
      <label><span>Your name</span><input type="text" bind:value={displayName} autocomplete="name" maxlength="100" /></label>
    {:else}
      {#if desktop}
        <p class="muted">The address you open Streamline at in the browser. You sign in once; this computer then keeps its own key in the system keyring.</p>
        <label><span>Server</span><input type="text" bind:value={server} placeholder="streamline.lan:3000" autocapitalize="none" spellcheck="false" required /></label>
      {/if}
      <label><span>Username</span><input type="text" bind:value={username} autocomplete="username" autocapitalize="none" required /></label>
      {#if setup}<label><span>Your name</span><input type="text" bind:value={displayName} autocomplete="name" /></label>{/if}
      <label>
        <span>Password</span>
        <input type="password" bind:value={password} autocomplete={setup ? 'new-password' : 'current-password'} minlength={setup ? 8 : undefined} required />
      </label>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <button class="btn primary" type="submit" disabled={busy}>
      {setup ? 'Create account' : desktop ? (choice === 'local' ? (busy ? 'Starting…' : 'Start') : busy ? 'Connecting…' : 'Connect') : 'Sign in'}
    </button>
  </form>
</main>

<style>
  .choice {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    padding: 4px;
    border-radius: var(--radius-sm);
    background: var(--bg);
  }
  .choice button {
    padding: 8px 6px;
    border-radius: var(--radius-sm);
    font-size: 14px;
    font-weight: 600;
    color: var(--muted);
  }
  .choice button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  main {
    min-height: 100dvh;
    display: grid;
    place-items: center;
    padding: 16px;
  }
  form {
    width: min(380px, 100%);
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    box-shadow: var(--shadow);
  }
  .logo {
    display: flex;
  }
  h1 {
    font-size: 22px;
  }
  p {
    margin: 0;
    font-size: 14px;
  }
  label span {
    display: block;
    font-size: 13px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .error {
    color: var(--danger);
  }
  .btn {
    margin-top: 6px;
    min-height: 44px;
  }
</style>
