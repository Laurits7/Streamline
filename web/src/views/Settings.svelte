<script lang="ts">
  import { api, ApiError } from '../lib/api/client'
  import type { ApiToken } from '../lib/api/types/ApiToken'
  import type { Me } from '../lib/api/types/Me'
  import type { UserSummary } from '../lib/api/types/UserSummary'
  import Icon from '../lib/components/Icon.svelte'
  import PlaceSwitcher from '../lib/components/PlaceSwitcher.svelte'
  import { store } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'

  let { onlogout }: { onlogout: () => void } = $props()

  const me = $derived(store.me!)
  const zones: string[] = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf?.('timeZone') ?? []

  const locales = ['', 'en-GB', 'en-US', 'et-EE', 'fi-FI', 'sv-SE', 'de-DE', 'nl-NL', 'fr-FR', 'es-ES', 'it-IT', 'pl-PL', 'lv-LV', 'lt-LT', 'ru-RU']
  const localeName = (l: string) => {
    if (!l) return 'Browser default'
    try {
      return `${new Intl.DisplayNames([l], { type: 'language' }).of(l)} (${l})`
    } catch {
      return l
    }
  }
  const sample = $derived(
    new Date().toLocaleDateString(me.locale || undefined, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }),
  )
  let newPlace = $state('')
  let newGroup = $state('')
  let people = $state<UserSummary[]>([])
  async function loadPeople() {
    if (!people.length) people = await store.directory().catch(() => [])
  }
  const canLocate = typeof window !== 'undefined' && window.isSecureContext && 'geolocation' in navigator
  function locate(id: string) {
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        store.updatePlace(id, { lat: pos.coords.latitude, lon: pos.coords.longitude })
        toast('Location saved')
      },
      () => toast('Could not get your location', 'error'),
      { enableHighAccuracy: true, timeout: 20_000 },
    )
  }
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
  <div class="grid2">
    <label>
      <span>Language for dates</span>
      <select value={me.locale} onchange={(e) => store.updateMe({ locale: (e.currentTarget as HTMLSelectElement).value })}>
        {#if !locales.includes(me.locale)}<option value={me.locale}>{me.locale}</option>{/if}
        {#each locales as l (l)}<option value={l}>{localeName(l)}</option>{/each}
      </select>
    </label>
    <label>
      <span>Week starts on</span>
      <select value={me.week_start} onchange={(e) => store.updateMe({ week_start: Number((e.currentTarget as HTMLSelectElement).value) })}>
        <option value={1}>Monday</option>
        <option value={7}>Sunday</option>
        <option value={6}>Saturday</option>
      </select>
    </label>
  </div>
  <p class="help muted">Example: {sample}. The week start is used by weekly routines and views.</p>
  <p class="help muted">
    Unfinished tasks are carried over or marked missed when your day ends. A time after midnight (e.g. 04:00) keeps
    late evenings on the same day.
  </p>
</section>

<section class="card">
  <h2>Daily planning</h2>
  <label>
    <span>When do you plan?</span>
    <select value={me.plan_mode} onchange={(e) => store.updateMe({ plan_mode: (e.currentTarget as HTMLSelectElement).value as 'evening' | 'morning' | 'both' })}>
      <option value="evening">In the evening: plan tomorrow</option>
      <option value="morning">In the morning: plan today</option>
      <option value="both">Both: plan in the evening, check in the morning</option>
    </select>
  </label>
  <div class="grid2">
    {#if me.plan_mode !== 'morning'}
      <label>
        <span>Evening reminder</span>
        <input type="time" value={me.plan_time_evening} onchange={(e) => store.updateMe({ plan_time_evening: (e.currentTarget as HTMLInputElement).value })} />
      </label>
    {/if}
    {#if me.plan_mode !== 'evening'}
      <label>
        <span>Morning reminder</span>
        <input type="time" value={me.plan_time_morning} onchange={(e) => store.updateMe({ plan_time_morning: (e.currentTarget as HTMLInputElement).value })} />
      </label>
    {/if}
  </div>
  <div class="grid2">
    <label>
      <span>My day starts</span>
      <input type="time" value={me.day_window_start} onchange={(e) => store.updateMe({ day_window_start: (e.currentTarget as HTMLInputElement).value })} />
    </label>
    <label>
      <span>My day ends</span>
      <input type="time" value={me.day_window_end} onchange={(e) => store.updateMe({ day_window_end: (e.currentTarget as HTMLInputElement).value })} />
    </label>
  </div>
  <p class="help muted">
    You get one reminder at the planning time (in the app; phone notifications come later), and Today shows a
    banner until the day is planned. “My day” is the time counted as free when planning.
  </p>
</section>

<section class="card">
  <h2>Groups</h2>
  <p class="help muted">
    Share projects, tasks and routines with a group (e.g. your family). Everyone in the group sees them, and anyone can
    complete them. Your day plans, focus time and notes stay your own.
  </p>
  {#each store.myGroups() as g (g.id)}
    {@const mine = g.members.find((m) => m.user_id === me.id)}
    {@const canManage = mine?.role === 'owner' || me.is_admin}
    <div class="group card">
      <div class="line top">
        {#if canManage}
          <input class="place-name" type="text" value={g.name} onchange={(e) => store.renameGroup(g.id, (e.currentTarget as HTMLInputElement).value)} aria-label="Group name" />
        {:else}<strong>{g.name}</strong>{/if}
        <div class="row">
          <button class="btn small" onclick={() => confirm(`Leave “${g.name}”? Its shared things disappear from your lists.`) && store.removeMember(g.id, me.id)}>Leave</button>
          {#if canManage}<button class="btn small danger" onclick={() => confirm(`Delete “${g.name}”?`) && store.deleteGroup(g.id)}>Delete</button>{/if}
        </div>
      </div>
      <ul class="members">
        {#each g.members as m (m.user_id)}
          <li>
            <Icon name="users" size={14} /> {m.display_name} <span class="muted small">@{m.username}{m.role === 'owner' ? ' · owner' : ''}</span>
            {#if canManage && m.user_id !== me.id}
              <button class="link" onclick={() => store.addMember(g.id, m.user_id, m.role === 'owner' ? 'member' : 'owner')}>{m.role === 'owner' ? 'make member' : 'make owner'}</button>
              <button class="link danger" onclick={() => store.removeMember(g.id, m.user_id)}>remove</button>
            {/if}
          </li>
        {/each}
      </ul>
      {#if canManage}
        <select
          value=""
          aria-label="Add someone to {g.name}"
          onfocus={loadPeople}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLSelectElement).value
            if (v) store.addMember(g.id, v)
            ;(e.currentTarget as HTMLSelectElement).value = ''
          }}>
          <option value="">+ Add someone…</option>
          {#each people.filter((p) => !g.members.some((m) => m.user_id === p.id)) as p (p.id)}<option value={p.id}>{p.display_name} (@{p.username})</option>{/each}
        </select>
      {/if}
    </div>
  {/each}
  <form class="row" onsubmit={(e) => { e.preventDefault(); if (newGroup.trim()) { store.createGroup(newGroup.trim()); newGroup = '' } }}>
    <input type="text" bind:value={newGroup} placeholder="New group, e.g. Family" />
    <button class="btn" type="submit">Create</button>
  </form>
</section>

<section class="card">
  <h2>Places</h2>
  <p class="help muted">
    Where tasks are done (e.g. Home, Cottage, Town). Pick where you are with the 📍 switcher and lists show only what
    can be done there (plus tasks without a place).
  </p>
  {#if store.placeList().length}
    <div class="where">
      <span>I'm currently at</span>
      <PlaceSwitcher />
    </div>
  {/if}
  {#each store.placeList() as p (p.id)}
    <div class="line">
      <div>
        <input class="place-name" type="text" value={p.name} onchange={(e) => store.updatePlace(p.id, { name: (e.currentTarget as HTMLInputElement).value })} aria-label="Place name" />
        <div class="muted small">
          {#if p.lat !== null}Location saved · within
            <select class="radius" value={p.radius_m} onchange={(e) => store.updatePlace(p.id, { radius_m: Number((e.currentTarget as HTMLSelectElement).value) })}>
              {#each [100, 200, 500, 1000, 3000] as r (r)}<option value={r}>{r < 1000 ? `${r} m` : `${r / 1000} km`}</option>{/each}
            </select>
          {:else}No location (GPS can't detect it){/if}
        </div>
      </div>
      <div class="row">
        {#if canLocate}<button class="btn small" onclick={() => locate(p.id)}>Use my location</button>{/if}
        <button class="btn small danger" onclick={() => confirm(`Delete “${p.name}”? Its tasks become “anywhere”.`) && store.deletePlace(p.id)}>Delete</button>
      </div>
    </div>
  {/each}
  <form class="row" onsubmit={(e) => { e.preventDefault(); if (newPlace.trim()) { store.createPlace(newPlace.trim()); newPlace = '' } }}>
    <input type="text" bind:value={newPlace} placeholder="New place, e.g. Cottage" />
    <button class="btn" type="submit">Add</button>
  </form>
  <label class="check gps">
    <input type="checkbox" checked={store.useGps} disabled={!canLocate} onchange={(e) => store.setUseGps((e.currentTarget as HTMLInputElement).checked)} />
    Detect my place by GPS on this device
  </label>
  <p class="help muted">
    {#if canLocate}Uses places with a saved location; your position stays on this device.{:else}Browsers only allow location access over HTTPS, so this is available when Streamline is opened through HTTPS (see the README).{/if}
  </p>
</section>

<section class="card">
  <h2>Focus timer</h2>
  <div class="grid4">
    <label><span>Focus (min)</span><input type="number" min="1" max="180" value={me.focus_work_min} onchange={(e) => store.updateMe({ focus_work_min: Number((e.currentTarget as HTMLInputElement).value) })} /></label>
    <label><span>Short break</span><input type="number" min="1" max="60" value={me.focus_short_break_min} onchange={(e) => store.updateMe({ focus_short_break_min: Number((e.currentTarget as HTMLInputElement).value) })} /></label>
    <label><span>Long break</span><input type="number" min="1" max="120" value={me.focus_long_break_min} onchange={(e) => store.updateMe({ focus_long_break_min: Number((e.currentTarget as HTMLInputElement).value) })} /></label>
    <label><span>Long break every</span><input type="number" min="1" max="12" value={me.focus_long_every} onchange={(e) => store.updateMe({ focus_long_every: Number((e.currentTarget as HTMLInputElement).value) })} /></label>
  </div>
  <label class="check">
    <input type="checkbox" checked={store.pref('focus_sound', true)} onchange={(e) => store.setPref('focus_sound', (e.currentTarget as HTMLInputElement).checked)} />
    Play a sound when an interval ends
  </label>
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
  .group {
    padding: 10px 12px;
    margin-bottom: 10px;
  }
  .group .top {
    border: 0;
    padding: 0 0 6px;
  }
  .members {
    list-style: none;
    padding: 0;
    margin: 0 0 8px;
  }
  .members li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 3px 0;
    font-size: 14px;
  }
  .link {
    color: var(--accent);
    font-size: 12px;
  }
  .link.danger {
    color: var(--danger);
  }
  .where {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
    font-size: 14px;
  }
  .place-name {
    padding: 4px 8px !important;
    font-weight: 600;
    max-width: 220px;
  }
  .radius {
    width: auto !important;
    padding: 0 4px !important;
    font-size: 12px;
  }
  .gps {
    margin-top: 12px;
  }
  .grid4 {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
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
