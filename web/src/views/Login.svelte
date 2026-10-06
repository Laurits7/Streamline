<script lang="ts">
  import { api, ApiError } from '../lib/api/client'

  let { setup, onlogin }: { setup: boolean; onlogin: () => void } = $props()
  let username = $state('')
  let password = $state('')
  let displayName = $state('')
  let error = $state('')
  let busy = $state(false)

  async function submit(e: Event) {
    e.preventDefault()
    busy = true
    error = ''
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
      <svg viewBox="0 0 64 64" width="44" height="44"><rect width="64" height="64" rx="14" fill="var(--accent)" /><path d="M18 34l9 9 19-21" fill="none" stroke="var(--accent-text)" stroke-width="6" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </div>
    <h1>{setup ? 'Welcome to Streamline' : 'Sign in'}</h1>
    {#if setup}<p class="muted">Create the admin account for this household. You can add more people later in Settings.</p>{/if}
    <label><span>Username</span><input type="text" bind:value={username} autocomplete="username" autocapitalize="none" required /></label>
    {#if setup}<label><span>Your name</span><input type="text" bind:value={displayName} autocomplete="name" /></label>{/if}
    <label>
      <span>Password</span>
      <input type="password" bind:value={password} autocomplete={setup ? 'new-password' : 'current-password'} minlength={setup ? 8 : undefined} required />
    </label>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <button class="btn primary" type="submit" disabled={busy}>{setup ? 'Create account' : 'Sign in'}</button>
  </form>
</main>

<style>
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
