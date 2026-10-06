<script lang="ts">
  import { api, ApiError } from '../lib/api/client'
  import type { ApiToken } from '../lib/api/types/ApiToken'
  import type { Backup } from '../lib/api/types/Backup'
  import type { DayTemplate } from '../lib/api/types/DayTemplate'
  import type { TemplateBlock } from '../lib/api/types/TemplateBlock'
  import type { Me } from '../lib/api/types/Me'
  import type { UserSummary } from '../lib/api/types/UserSummary'
  import Icon from '../lib/components/Icon.svelte'
  import PlaceSwitcher from '../lib/components/PlaceSwitcher.svelte'
  import { disablePush, enablePush, pushBlocker, pushEnabled } from '../lib/pwa'
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
  // Backups (admins)
  let backups = $state<Backup[]>([])
  let backingUp = $state(false)
  $effect(() => {
    if (store.me?.is_admin) api.get<Backup[]>('/admin/backups').then((b) => (backups = b)).catch(() => {})
  })
  async function backupNow() {
    backingUp = true
    try {
      await api.post('/admin/backups')
      backups = await api.get<Backup[]>('/admin/backups')
      toast('Backup saved')
    } catch (e) {
      err(e)
    } finally {
      backingUp = false
    }
  }
  const mb = (n: number) => (n < 1e6 ? `${Math.round(n / 1024)} KB` : `${(n / 1e6).toFixed(1)} MB`)

  // Notifications
  const KINDS = [
    ['planning', 'Planning reminders'],
    ['ready', 'A waiting task is ready'],
    ['focus', 'Focus timer: interval over'],
    ['conflict', 'Overlaps with your calendar'],
    ['metric', 'Tracking reminders'],
  ] as const
  let pushOn = $state(false)
  let pushBusy = $state(false)
  const blocker = pushBlocker()
  $effect(() => {
    pushEnabled().then((v) => (pushOn = v)).catch(() => {})
  })
  async function togglePush() {
    pushBusy = true
    try {
      if (pushOn) await disablePush()
      else await enablePush()
      pushOn = await pushEnabled()
      toast(pushOn ? 'Notifications are on for this device' : 'Notifications are off for this device')
    } catch (e) {
      toast(e instanceof Error ? e.message : 'Could not change notifications', 'error')
    } finally {
      pushBusy = false
    }
  }
  async function testPush() {
    try {
      const r = await api.post<{ devices: number; ntfy: boolean }>('/push/test')
      toast(r.devices || r.ntfy ? `Test sent to ${r.devices} device${r.devices === 1 ? '' : 's'}${r.ntfy ? ' and ntfy' : ''}` : 'No device has notifications on yet')
    } catch (e) {
      err(e)
    }
  }
  function toggleKind(kind: string, on: boolean) {
    const off = new Set(me.notify_off)
    if (on) off.delete(kind)
    else off.add(kind)
    store.updateMe({ notify_off: [...off] })
  }

  // Tracking
  let nm = $state({ name: '', kind: 'number', unit: '' })
  function addMetric(e: Event) {
    e.preventDefault()
    if (!nm.name.trim()) return
    store.createMetric({ name: nm.name.trim(), kind: nm.kind, unit: nm.unit.trim() })
    nm = { name: '', kind: 'number', unit: '' }
  }
  async function importCsv(metricId: string, file: File | undefined) {
    if (!file) return
    try {
      const text = await file.text()
      const r = await api.post<{ imported: number }>(`/metrics/${metricId}/import`, { text })
      toast(`Imported ${r.imported} values`)
      await store.sync()
    } catch (e) {
      err(e)
    }
  }

  // Day templates
  const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
  function newTemplate() {
    store.createTemplate(store.dayTemplates.size ? 'New template' : 'Workday', store.dayTemplates.size ? [] : [1, 2, 3, 4, 5], [
      { title: 'Deep work', start: '09:00', end: '12:00', energy: 'hard' },
      { title: 'Admin and errands', start: '13:00', end: '15:00', energy: 'easy' },
    ])
  }
  function setBlocks(t: DayTemplate, i: number, patch: Partial<TemplateBlock> | null) {
    const blocks =
      patch === null ? t.blocks.filter((_, j) => j !== i) : t.blocks.map((b, j) => (j === i ? { ...b, ...patch } : b))
    store.updateTemplate(t.id, { blocks })
  }
  function addTemplateBlock(t: DayTemplate) {
    const last = t.blocks.at(-1)
    const start = last && last.end < '22:00' ? last.end : '09:00'
    const h = Math.min(23, Number(start.slice(0, 2)) + 1)
    store.updateTemplate(t.id, { blocks: [...t.blocks, { title: 'Chores', start, end: `${String(h).padStart(2, '0')}:${start.slice(3)}`, energy: null }] })
  }
  function toggleDay(t: DayTemplate, d: number) {
    store.updateTemplate(t.id, { weekdays: t.weekdays.includes(d) ? t.weekdays.filter((x) => x !== d) : [...t.weekdays, d].sort() })
  }

  const err = (e: unknown) => toast(e instanceof ApiError ? e.message : 'Something went wrong', 'error')

  // Calendar (CalDAV). The password is write-only: the server never sends it back.
  let cal = $state({ url: '', username: '', password: '' })
  $effect(() => {
    const a = store.calendarAccount
    if (a && !cal.url) cal = { url: a.url, username: a.username, password: '' }
  })
  let calBusy = $state<'' | 'test' | 'save' | 'sync'>('')
  let calTest = $state<{ ok: boolean; calendars: string[]; error: string | null } | null>(null)
  const calendars = $derived([...store.calendars.values()].sort((a, b) => a.name.localeCompare(b.name)))
  const calInput = () => ({ url: cal.url.trim(), username: cal.username.trim(), password: cal.password || undefined })
  async function calRun(kind: 'test' | 'save' | 'sync', fn: () => Promise<void>) {
    calBusy = kind
    try {
      await fn()
    } catch (e) {
      err(e)
    } finally {
      calBusy = ''
    }
  }
  const testCal = () => calRun('test', async () => void (calTest = await store.testCalendar(calInput())))
  const saveCal = (e: Event) => {
    e.preventDefault()
    calRun('save', async () => {
      const a = await store.saveCalendarAccount(calInput())
      cal.password = ''
      calTest = null
      if (a?.status === 'ok') toast('Calendar connected')
      else toast('Saved, but the calendar could not be read (see below)', 'error')
    })
  }
  const syncCal = () => calRun('sync', () => store.syncCalendar())
  async function disconnectCal() {
    if (!confirm('Disconnect the calendar? Its events disappear from Streamline (nothing changes on the server).')) return
    await store.disconnectCalendar().catch(err)
    cal = { url: '', username: '', password: '' }
  }
  const when = (ts: string | null) =>
    ts ? new Date(ts).toLocaleString(me.locale || undefined, { dateStyle: 'medium', timeStyle: 'short' }) : 'never'

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

<div class="settings-grid">
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

<section class="card" id="notifications">
  <h2>Notifications</h2>
  <p class="help muted">
    Reminders and alerts show inside Streamline while it's open. Turn on notifications to get them on this device even
    when Streamline is closed: on a phone they appear on the lock screen like any app's.
  </p>
  <div class="line">
    <div>
      <strong>This device</strong>
      <div class="muted small">{blocker ?? (pushOn ? 'Notifications are on.' : 'Notifications are off.')}</div>
    </div>
    <div class="row">
      {#if !blocker}<button class="btn small" class:primary={!pushOn} disabled={pushBusy} onclick={togglePush}>{pushBusy ? (pushOn ? 'Turning off…' : 'Turning on…') : pushOn ? 'Turn off' : 'Turn on'}</button>{/if}
      <button class="btn small" onclick={testPush}>Send a test</button>
    </div>
  </div>
  <p class="help muted kinds-title">What to notify about (all devices):</p>
  {#each KINDS as [kind, label] (kind)}
    <label class="check plain"><input type="checkbox" checked={!me.notify_off.includes(kind)} onchange={(e) => toggleKind(kind, (e.currentTarget as HTMLInputElement).checked)} /> {label}</label>
  {/each}
  <label class="ntfy">
    <span>Also send to ntfy (optional, works without HTTPS): your topic's address</span>
    <input type="url" value={me.ntfy_url ?? ''} placeholder="https://ntfy.sh/your-secret-topic" onchange={(e) => store.updateMe({ ntfy_url: (e.currentTarget as HTMLInputElement).value })} />
  </label>
</section>

<section class="card" id="tracking">
  <h2>Tracking</h2>
  <p class="help muted">
    What you log on the Today view. Mood and weight are built in; add your own (sleep hours, water, steps, “took
    vitamins”…). A reminder time sends a notice if nothing is logged by then. Only you can see your tracking and journal.
  </p>
  <label class="inline">
    <span>Units</span>
    <select value={me.unit_system} onchange={(e) => store.updateMe({ unit_system: (e.currentTarget as HTMLSelectElement).value as 'metric' | 'imperial' })}>
      <option value="metric">Metric (kg)</option>
      <option value="imperial">Imperial (lb)</option>
    </select>
  </label>
  {#each store.metricList(true) as m (m.id)}
    <div class="line metric-row" class:archived={m.archived}>
      <div class="mname">
        <input class="place-name" type="text" value={m.name} aria-label="Name" onchange={(e) => store.updateMetric(m.id, { name: (e.currentTarget as HTMLInputElement).value })} />
        <span class="muted small">{m.kind === 'yes_no' ? 'yes/no' : m.kind}{m.key === 'weight' ? (me.unit_system === 'imperial' ? ' · lb' : ' · kg') : m.unit ? ` · ${m.unit}` : ''}</span>
      </div>
      <div class="row">
        <select class="small-select" value={m.aggregate} aria-label="Several a day" title="When logged several times a day" onchange={(e) => store.updateMetric(m.id, { aggregate: (e.currentTarget as HTMLSelectElement).value as 'latest' })}>
          <option value="latest">latest</option>
          <option value="average">average</option>
          <option value="sum">sum</option>
          <option value="max">highest</option>
        </select>
        <input class="small-time" type="time" value={m.reminder_time ?? ''} aria-label="Reminder" title="Remind me if not logged by" onchange={(e) => store.updateMetric(m.id, { reminder_time: (e.currentTarget as HTMLInputElement).value || null })} />
        <a class="btn small" href="/api/v1/metrics/{m.id}/csv" download>CSV</a>
        <label class="btn small file">Import<input type="file" accept=".csv,text/csv,text/plain" onchange={(e) => importCsv(m.id, (e.currentTarget as HTMLInputElement).files?.[0])} /></label>
        <button class="btn small" onclick={() => store.updateMetric(m.id, { archived: !m.archived })}>{m.archived ? 'Show' : 'Hide'}</button>
        {#if !m.key}<button class="btn small danger" onclick={() => confirm(`Delete “${m.name}” and everything logged for it?`) && store.deleteMetric(m.id)}>Delete</button>{/if}
      </div>
    </div>
  {/each}
  <form class="row" onsubmit={addMetric}>
    <input type="text" bind:value={nm.name} placeholder="New metric, e.g. Sleep" aria-label="New metric name" />
    <select bind:value={nm.kind} aria-label="Kind">
      <option value="number">Number</option>
      <option value="scale">Scale 1–5</option>
      <option value="yes_no">Yes / no</option>
    </select>
    {#if nm.kind === 'number'}<input class="unit" type="text" bind:value={nm.unit} placeholder="unit, e.g. h" aria-label="Unit" />{/if}
    <button class="btn" type="submit">Add</button>
  </form>
  <p class="help muted export">
    Import takes CSV rows of <code>date,value</code> (weight in kg). <a href="/api/v1/export" download>Download all your data (JSON)</a>
  </p>
</section>

<section class="card" id="templates">
  <h2>Day templates</h2>
  <p class="help muted">
    Split days into time blocks with a theme (deep work, admin, chores…). A template is used automatically on its
    weekdays; any day can also use another template, or have its blocks changed, from the timeline's <em>Blocks…</em>
    menu. <em>Suggest times</em> in the planner puts hard tasks in “hard” blocks and easy ones in “easy” blocks.
  </p>
  {#each store.templateList() as t (t.id)}
    <div class="group card">
      <div class="line top">
        <input class="place-name" type="text" value={t.name} aria-label="Template name" onchange={(e) => store.updateTemplate(t.id, { name: (e.currentTarget as HTMLInputElement).value })} />
        <button class="btn small danger" onclick={() => confirm(`Delete “${t.name}”? Days that used it keep their blocks.`) && store.deleteTemplate(t.id)}>Delete</button>
      </div>
      <div class="days" role="group" aria-label="Weekdays for {t.name}">
        {#each WEEKDAYS as d, i (d)}
          <button class="chip" class:on={t.weekdays.includes(i + 1)} aria-pressed={t.weekdays.includes(i + 1)} onclick={() => toggleDay(t, i + 1)}>{d}</button>
        {/each}
      </div>
      {#each t.blocks as b, i (i)}
        <div class="tblock">
          <input type="text" value={b.title} aria-label="Block theme" onchange={(e) => setBlocks(t, i, { title: (e.currentTarget as HTMLInputElement).value })} />
          <input type="time" value={b.start} aria-label="From" onchange={(e) => setBlocks(t, i, { start: (e.currentTarget as HTMLInputElement).value })} />
          <input type="time" value={b.end} aria-label="To" onchange={(e) => setBlocks(t, i, { end: (e.currentTarget as HTMLInputElement).value })} />
          <select value={b.energy ?? ''} aria-label="Kind of work" onchange={(e) => setBlocks(t, i, { energy: (e.currentTarget as HTMLSelectElement).value || null })}>
            <option value="">Any work</option>
            <option value="hard">Hard tasks</option>
            <option value="medium">Medium tasks</option>
            <option value="easy">Easy tasks</option>
          </select>
          <button class="icon-btn" aria-label="Remove block" onclick={() => setBlocks(t, i, null)}><Icon name="x" size={14} /></button>
        </div>
      {/each}
      <button class="btn small" onclick={() => addTemplateBlock(t)}><Icon name="plus" size={14} /> Block</button>
    </div>
  {/each}
  <button class="btn" onclick={newTemplate}><Icon name="plus" size={16} /> New template</button>
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

<a class="card help-link" href="/summary"><Icon name="book" /><span>Summary &amp; journal</span><Icon name="right" size={16} /></a>
<a class="card help-link" href="/goals"><Icon name="flag" /><span>Goals</span><Icon name="right" size={16} /></a>
<a class="card help-link" href="/trends"><Icon name="chart" /><span>Trends</span><Icon name="right" size={16} /></a>
<a class="card help-link" href="/occasions"><Icon name="gift" /><span>Namedays & birthdays</span><Icon name="right" size={16} /></a>

<section class="card" id="calendar">
  <h2>Calendar</h2>
  <p class="help muted">
    Show your calendar's events in the day view, so plans fit around them. Works with any CalDAV calendar (Nextcloud,
    Fastmail, iCloud with an app-specific password, mailbox.org, Radicale, Baïkal…), the same details you would give
    Evolution or Thunderbird. Read-only: Streamline never changes your calendar. Google Calendar needs OAuth sign-in,
    which isn't supported yet.
  </p>
  <form onsubmit={saveCal}>
    <label><span>CalDAV URL</span><input type="url" bind:value={cal.url} placeholder="https://cloud.example.org/remote.php/dav/" required autocomplete="url" /></label>
    <div class="grid2">
      <label><span>Username</span><input type="text" bind:value={cal.username} autocomplete="username" /></label>
      <label>
        <span>Password{store.calendarAccount?.has_password ? ' (saved; leave empty to keep)' : ''}</span>
        <input type="password" bind:value={cal.password} autocomplete="new-password" placeholder={store.calendarAccount?.has_password ? '••••••••' : ''} />
      </label>
    </div>
    <div class="row">
      <button class="btn" type="button" onclick={testCal} disabled={!!calBusy || !cal.url.trim()}>{calBusy === 'test' ? 'Testing…' : 'Test connection'}</button>
      <button class="btn primary" type="submit" disabled={!!calBusy}>{calBusy === 'save' ? 'Connecting…' : store.calendarAccount ? 'Save' : 'Connect'}</button>
    </div>
  </form>
  {#if calTest}
    <p class="cal-status" class:bad={!calTest.ok} role="status">
      {#if calTest.ok}Connection works. Found {calTest.calendars.length ? calTest.calendars.join(', ') : 'no calendars'}.{:else}{calTest.error}{/if}
    </p>
  {/if}
  {#if store.calendarAccount}
    {@const a = store.calendarAccount}
    <p class="cal-status" class:bad={a.status === 'error'} role="status">
      {#if a.status === 'error'}
        Last sync failed: {a.last_error}. Showing the events from the last good sync.
      {:else if a.status === 'ok'}
        Synced {when(a.last_sync_at)}. Checks for changes every 15 minutes.
      {:else}Not synced yet.{/if}
    </p>
    {#each calendars as c (c.id)}
      <div class="line">
        <label class="check plain">
          <input type="checkbox" checked={c.enabled} onchange={(e) => store.updateCalendar(c.id, { enabled: (e.currentTarget as HTMLInputElement).checked })} />
          {c.name}
        </label>
        <label class="check plain small" title="Count all-day events of this calendar as busy for the whole day">
          <input type="checkbox" checked={c.all_day_busy} onchange={(e) => store.updateCalendarAllDay(c.id, (e.currentTarget as HTMLInputElement).checked)} />
          all-day = busy
        </label>
        <input
          class="swatch"
          type="color"
          value={c.user_color || c.color || '#64748b'}
          aria-label="Colour of {c.name}"
          onchange={(e) => store.updateCalendar(c.id, { user_color: (e.currentTarget as HTMLInputElement).value })} />
      </div>
    {/each}
    <div class="row">
      <button class="btn small" onclick={syncCal} disabled={!!calBusy}>{calBusy === 'sync' ? 'Syncing…' : 'Sync now'}</button>
      <button class="btn small danger" onclick={disconnectCal}>Disconnect</button>
    </div>
  {/if}
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
  <section class="card" id="backups">
    <h2>Backups</h2>
    <p class="help muted">
      The database is copied to <code>data/backups/</code> every day (the newest 7 are kept). Keep a copy of the
      <code>data/</code> folder somewhere else too; restoring is described in the README.
    </p>
    {#each backups as b (b.name)}
      <div class="line">
        <span>{new Date(b.created_at).toLocaleString(me.locale || undefined, { dateStyle: 'medium', timeStyle: 'short' })} <span class="muted small">· {mb(b.bytes)}</span></span>
        <a class="btn small" href="/api/v1/admin/backups/{b.name}" download>Download</a>
      </div>
    {:else}<p class="muted small">No backups yet.</p>{/each}
    <button class="btn" onclick={backupNow} disabled={backingUp}>{backingUp ? 'Backing up…' : 'Back up now'}</button>
  </section>
{/if}

<section class="card">
  <button class="btn" onclick={onlogout}><Icon name="logout" size={16} /> Sign out</button>
</section>
</div>

<style>
  /* Wide screens: two columns of sections instead of very long form fields. */
  @media (min-width: 1280px) {
    .settings-grid {
      columns: 2 420px;
      column-gap: 14px;
    }
    .settings-grid > :global(*) {
      break-inside: avoid;
    }
  }
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
  .kinds-title {
    margin: 12px 0 4px;
  }
  .ntfy {
    margin-top: 12px;
  }
  #notifications .check.plain {
    padding: 3px 0;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .inline span {
    margin: 0;
  }
  .inline select {
    width: auto;
  }
  .metric-row {
    flex-wrap: wrap;
  }
  .metric-row .row {
    flex-wrap: wrap;
  }
  .metric-row.archived {
    opacity: 0.55;
  }
  .mname {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .small-select,
  .small-time {
    width: auto !important;
    padding: 2px 6px !important;
    font-size: 12px;
  }
  .file {
    position: relative;
    overflow: hidden;
  }
  .file input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  .unit {
    max-width: 120px;
  }
  .export {
    margin-top: 12px;
  }
  .days {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 4px 0 10px;
  }
  .chip {
    font-size: 12px;
    padding: 3px 9px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .chip.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .tblock {
    display: grid;
    grid-template-columns: minmax(120px, 1fr) 124px 124px minmax(110px, 140px) 28px;
    gap: 6px;
    align-items: center;
    margin-bottom: 6px;
  }
  @media (max-width: 560px) {
    .tblock {
      grid-template-columns: 1fr 1fr;
    }
    .tblock input[type='text'] {
      grid-column: 1 / -1;
    }
  }
  .cal-status {
    font-size: 13px;
    margin: 10px 0 4px;
    color: var(--muted);
  }
  .cal-status.bad {
    color: var(--danger);
  }
  .check.plain {
    padding: 0;
    margin: 0;
  }
  .swatch {
    width: 36px !important;
    height: 28px;
    padding: 0 !important;
    border: 0;
    background: none;
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
