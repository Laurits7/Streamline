<script lang="ts">
  import { api, ApiError } from '../lib/api/client'
  import type { ApiToken } from '../lib/api/types/ApiToken'
  import type { Me } from '../lib/api/types/Me'
  import Icon from '../lib/components/Icon.svelte'
  import { store } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'

  let { onlogout }: { onlogout: () => void } = $props()

  const me = $derived(store.me!)
  const zones: string[] = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf?.('timeZone') ?? []

  const err = (e: unknown) => toast(e instanceof ApiError ? e.message : 'Something went wrong', 'error')

  // Password
  let current = $state('')
  let next = $state('')
  async function changePassword(e: Event) {
    e.preventDefault()
    try {
      await api.post('/me/password', { current_password: current, new_password: next })
      current = next = ''
      toast('Password changed. Other devices were signed out.')
    } catch (e) {
      err(e)
    }
  }

  // API tokens
  let tokens = $state<ApiToken[]>([])
  let tokenName = $state('')
  let newToken = $state<string | null>(null)
  const loadTokens = async () => {
    try {
      tokens = await api.get<ApiToken[]>('/tokens')
    } catch (e) {
      err(e)
    }
  }
  async function createToken(e: Event) {
    e.preventDefault()
    if (!tokenName.trim()) return
    try {
      const r = await api.post<{ token: string; info: ApiToken }>('/tokens', { name: tokenName.trim() })
      newToken = r.token
      tokenName = ''
      await loadTokens()
    } catch (e) {
      err(e)
    }
  }
  async function revoke(t: ApiToken) {
    if (!confirm(`Revoke token “${t.name}”? Apps using it will be signed out.`)) return
    try {
      await api.del(`/tokens/${t.id}`)
      await loadTokens()
    } catch (e) {
      err(e)
    }
  }

  // Users (admin)
  let users = $state<Me[]>([])
  let nu = $state({ username: '', password: '', display_name: '', is_admin: false })
  const loadUsers = async () => {
    if (!store.me?.is_admin) return
    try {
      users = await api.get<Me[]>('/users')
    } catch (e) {
      err(e)
    }
  }
  async function addUser(e: Event) {
    e.preventDefault()
    try {
      await api.post('/users', nu)
      toast(`Created ${nu.username}`)
      nu = { username: '', password: '', display_name: '', is_admin: false }
      await loadUsers()
    } catch (e) {
      err(e)
    }
  }
  async function resetPassword(u: Me) {
    const pw = prompt(`New password for ${u.username} (min. 8 characters)`)
    if (!pw) return
    try {
      await api.patch(`/users/${u.id}`, { password: pw })
      toast('Password reset')
    } catch (e) {
      err(e)
    }
  }
  async function removeUser(u: Me) {
    if (!confirm(`Delete user ${u.username}?`)) return
    try {
      await api.del(`/users/${u.id}`)
      await loadUsers()
    } catch (e) {
      err(e)
    }
  }

  $effect(() => {
    loadTokens()
    loadUsers()
  })
</script>

<h1>Settings</h1>

<a class="card help-link" href="/help"><Icon name="help" /> <span>Help: what you can do and how</span><Icon name="right" size={16} /></a>

<section class="card">
  <h2>Profile</h2>
  <label>
    <span>Display name</span>
    <input type="text" value={me.display_name} onchange={(e) => store.updateMe({ display_name: (e.currentTarget as HTMLInputElement).value })} />
  </label>
  <div class="grid2">
    <label>
      <span>Timezone</span>
      <select value={me.timezone} onchange={(e) => store.updateMe({ timezone: (e.currentTarget as HTMLSelectElement).value })}>
        {#if !zones.includes(me.timezone)}<option value={me.timezone}>{me.timezone}</option>{/if}
        {#each zones as z (z)}<option value={z}>{z}</option>{/each}
      </select>
    </label>
    <label>
      <span>Day ends at</span>
      <input type="time" value={me.day_end} onchange={(e) => store.updateMe({ day_end: (e.currentTarget as HTMLInputElement).value })} />
    </label>
  </div>
  <p class="help muted">
    Unfinished tasks are carried over or marked missed when your day ends. A time after midnight (e.g. 04:00) keeps
    late evenings on the same day.
  </p>
</section>

<section class="card">
  <h2>Password</h2>
  <form onsubmit={changePassword}>
    <div class="grid2">
      <label><span>Current</span><input type="password" bind:value={current} autocomplete="current-password" required /></label>
      <label><span>New</span><input type="password" bind:value={next} autocomplete="new-password" minlength="8" required /></label>
    </div>
    <button class="btn" type="submit">Change password</button>
  </form>
</section>

<section class="card">
  <h2>API tokens</h2>
  <p class="help muted">For other apps and scripts: send <code>Authorization: Bearer &lt;token&gt;</code> to <code>/api/v1</code>.</p>
  {#if newToken}
    <div class="token-box">
      <p><strong>Copy this token now.</strong> It won't be shown again.</p>
      <code>{newToken}</code>
      <div class="row">
        <button class="btn small" onclick={() => navigator.clipboard?.writeText(newToken!).then(() => toast('Copied'))}>Copy</button>
        <button class="btn small" onclick={() => (newToken = null)}>Done</button>
      </div>
    </div>
  {/if}
  {#each tokens as t (t.id)}
    <div class="line">
      <div>
        <strong>{t.name}</strong>
        <div class="muted small">Created {new Date(t.created_at).toLocaleDateString()} · {t.last_used_at ? `last used ${new Date(t.last_used_at).toLocaleString()}` : 'never used'}</div>
      </div>
      <button class="btn small danger" onclick={() => revoke(t)}>Revoke</button>
    </div>
  {/each}
  <form class="row" onsubmit={createToken}>
    <input type="text" bind:value={tokenName} placeholder="Token name, e.g. “Phone widget”" />
    <button class="btn" type="submit">Create</button>
  </form>
</section>

{#if me.is_admin}
  <section class="card">
    <h2>Users</h2>
    {#each users as u (u.id)}
      <div class="line">
        <div>
          <strong>{u.display_name}</strong>
          <span class="muted small">@{u.username}{u.is_admin ? ' · admin' : ''}</span>
        </div>
        <div class="row">
          <button class="btn small" onclick={() => resetPassword(u)}>Reset password</button>
          {#if u.id !== me.id}<button class="btn small danger" onclick={() => removeUser(u)}>Delete</button>{/if}
        </div>
      </div>
    {/each}
    <form onsubmit={addUser} class="adduser">
      <div class="grid2">
        <label><span>Username</span><input type="text" bind:value={nu.username} required autocomplete="off" /></label>
        <label><span>Display name</span><input type="text" bind:value={nu.display_name} /></label>
        <label><span>Password</span><input type="password" bind:value={nu.password} minlength="8" required autocomplete="new-password" /></label>
        <label class="check"><input type="checkbox" bind:checked={nu.is_admin} /> Admin</label>
      </div>
      <button class="btn" type="submit">Add user</button>
    </form>
  </section>
{/if}

<section class="card">
  <button class="btn" onclick={onlogout}><Icon name="logout" size={16} /> Sign out</button>
</section>

<style>
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
    margin-bottom: 16px;
  }
  .help-link {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 18px;
    margin-bottom: 14px;
    color: var(--accent);
    font-weight: 600;
  }
  .help-link span {
    flex: 1;
  }
  section {
    padding: 18px;
    margin-bottom: 14px;
  }
  h2 {
    font-size: 16px;
    margin-bottom: 12px;
  }
  label {
    display: block;
    margin-bottom: 12px;
  }
  label span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .grid2 {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 0 12px;
  }
  .help {
    font-size: 13px;
    margin: 0 0 12px;
  }
  code {
    font-size: 12px;
    background: var(--surface-2);
    padding: 1px 5px;
    border-radius: 4px;
    word-break: break-all;
  }
  .token-box {
    background: var(--warn-soft);
    border-radius: var(--radius-sm);
    padding: 12px;
    margin-bottom: 12px;
  }
  .token-box p {
    margin: 0 0 8px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 8px;
  }
  .line {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }
  .small {
    font-size: 12px;
  }
  .adduser {
    margin-top: 16px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 22px;
  }
</style>
