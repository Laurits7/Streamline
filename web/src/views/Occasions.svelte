<script lang="ts">
  // Namedays and birthdays (SPEC §6.17): find names in the shared nameday calendar, keep a
  // list of your people, and choose what each kind of occasion creates.
  import type { NameMatch } from '../lib/api/types/NameMatch'
  import type { OccasionStep } from '../lib/api/types/OccasionStep'
  import Icon from '../lib/components/Icon.svelte'
  import { addDays, dayLabel, shortDate, userLocale } from '../lib/dates'
  import { fold, store } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'

  $effect(() => {
    store.loadNamedays().catch(() => {})
  })

  const md = (s: string) => {
    const [m, d] = s.slice(-5).split('-').map(Number)
    return new Date(Date.UTC(2024, m - 1, d)).toLocaleDateString(userLocale(), { day: 'numeric', month: 'short', timeZone: 'UTC' })
  }

  /** `MM-DD` dates of a name in the calendar. */
  function datesOf(name: string): string[] {
    const key = fold(name)
    const out: string[] = []
    for (const [date, names] of store.namedays?.byDate ?? []) if (names.some((n) => fold(n) === key)) out.push(date)
    return out.sort()
  }

  // ---- search ----
  let q = $state('')
  let results = $state<NameMatch[]>([])
  let timer: ReturnType<typeof setTimeout> | undefined
  $effect(() => {
    const term = q.trim()
    clearTimeout(timer)
    if (!term) {
      results = []
      return
    }
    timer = setTimeout(() => {
      store
        .searchNames(term)
        .then((r) => term === q.trim() && (results = r))
        .catch(() => {})
    }, 180)
  })
  const added = (name: string) => store.peopleList().some((p) => p.nameday_name && fold(p.nameday_name) === fold(name))
  function addName(m: NameMatch) {
    store.createPerson({ name: m.name, nameday_name: m.name })
    toast(`Added ${m.name}: nameday ${m.dates.map(md).join(', ')}`)
  }

  // ---- people ----
  let newName = $state('')
  let newBirthday = $state('')
  function addByBirthday(e: Event) {
    e.preventDefault()
    if (!newName.trim() || !newBirthday) return
    store.createPerson({ name: newName.trim(), birthday: newBirthday })
    newName = ''
    newBirthday = ''
  }
  const yearKnown = (b: string | null) => !!b && !b.startsWith('--')
  function setBirthday(id: string, value: string, known: boolean) {
    if (!value) return store.updatePerson(id, { birthday: null })
    store.updatePerson(id, { birthday: known ? value : `--${value.slice(5)}` })
  }

  // Coming up in the next 60 days.
  const upcoming = $derived.by(() => {
    if (!store.namedays && !store.people.size) return []
    const out: { date: string; text: string }[] = []
    for (let i = 0; i <= 60; i++) {
      const date = addDays(store.today, i)
      for (const o of store.occasionsOn(date))
        out.push({ date, text: `${o.person.name}: ${o.kind === 'nameday' ? 'nameday' : 'birthday'}` })
    }
    return out
  })

  // ---- templates ----
  const kinds = [
    ['birthday', 'Birthdays'],
    ['nameday', 'Namedays'],
  ] as const
  let drafts = $state<Record<string, OccasionStep[]>>({})
  const stepsOf = (kind: string) => drafts[kind] ?? store.occasionTemplates.get(kind)?.steps ?? []
  function edit(kind: string, i: number, patch: Partial<OccasionStep>) {
    const steps = stepsOf(kind).map((s, j) => (j === i ? { ...s, ...patch } : s))
    drafts = { ...drafts, [kind]: steps }
  }
  function addStep(kind: string) {
    const steps = [...stepsOf(kind), { title: 'Call {name}', offset_days: 0, task_type_id: 'tt_expires', after_previous: false }]
    drafts = { ...drafts, [kind]: steps }
  }
  function removeStep(kind: string, i: number) {
    const steps = stepsOf(kind).filter((_, j) => j !== i)
    if (steps.length) steps[0] = { ...steps[0], after_previous: false }
    drafts = { ...drafts, [kind]: steps }
  }
  async function save(kind: string) {
    await store.updateOccasionTemplate(kind, { steps: stepsOf(kind) }).catch(() => {})
    const { [kind]: _, ...rest } = drafts
    drafts = rest
    toast('Saved. New occasions use these steps.')
  }
  const types = $derived([...store.taskTypes.values()].sort((a, b) => a.name.localeCompare(b.name)))

  // ---- calendar (admins) ----
  let busy = $state(false)
  let uploadText = $state('')
  let uploadLabel = $state('')
  async function reload(upload?: { text: string; label: string }) {
    busy = true
    try {
      await store.reloadNamedays(upload)
      toast('Nameday calendar loaded')
      uploadText = ''
    } catch (e) {
      toast(e instanceof Error ? e.message : 'Could not load the list', 'error')
    } finally {
      busy = false
    }
  }
  const todays = $derived(store.namedaysOn(store.today))
</script>

<h1>Namedays & birthdays</h1>
<p class="lead muted">
  Pick the people whose namedays and birthdays you want to remember. A week ahead, Streamline adds tasks like “buy a
  present” and “wish them a happy birthday” to your inbox, each due on its day.
</p>

<div class="grid">
{#if upcoming.length}
  <section class="card">
    <h2>Coming up</h2>
    <ul class="upcoming">
      {#each upcoming as u (u.date + u.text)}
        <li><span class="when">{u.date > addDays(store.today, 1) ? `${dayLabel(u.date, store.today)} ${md(u.date)}` : shortDate(u.date, store.today)}</span> {u.text}</li>
      {/each}
    </ul>
  </section>
{/if}

<section class="card">
  <h2>Find a name</h2>
  {#if store.namedays && store.namedays.byDate.size}
    <input class="search" type="search" bind:value={q} placeholder="Type a name, e.g. Mari or Tonu" aria-label="Search the nameday calendar" />
    {#if results.length}
      <ul class="results">
        {#each results as r (r.name)}
          <li>
            <span><strong>{r.name}</strong> <span class="muted">{r.dates.map(md).join(', ')}</span></span>
            {#if added(r.name)}<span class="muted small"><Icon name="check" size={14} /> added</span>
            {:else}<button class="btn small" onclick={() => addName(r)}><Icon name="plus" size={14} /> Add</button>{/if}
          </li>
        {/each}
      </ul>
    {:else if q.trim()}
      <p class="muted small">No name matches “{q.trim()}”.</p>
    {/if}
    {#if todays.length}<p class="muted small">Today's namedays: {todays.join(', ')}</p>{/if}
  {:else if store.namedays}
    <p class="muted">The nameday calendar hasn't been loaded yet.{store.me?.is_admin ? ' Load it below.' : ' Ask an admin to load it in this page.'}</p>
  {:else}
    <p class="muted">Loading…</p>
  {/if}
</section>

<section class="card">
  <h2>Your people</h2>
  {#each store.peopleList() as p (p.id)}
    {@const nd = p.nameday_name ? datesOf(p.nameday_name) : []}
    <div class="person">
      <input class="name" type="text" value={p.name} aria-label="Name" onchange={(e) => store.updatePerson(p.id, { name: (e.currentTarget as HTMLInputElement).value })} />
      <div class="facts">
        {#if p.nameday_name}
          <span class="fact"><Icon name="flag" size={13} /> Nameday {nd.length ? nd.map(md).join(', ') : '(not in the calendar)'}{fold(p.nameday_name) !== fold(p.name) ? ` as ${p.nameday_name}` : ''}</span>
        {/if}
        <label class="fact">
          <Icon name="gift" size={13} /> Birthday
          <input
            type="date"
            value={p.birthday ? (yearKnown(p.birthday) ? p.birthday : `2000-${p.birthday.slice(2)}`) : ''}
            onchange={(e) => setBirthday(p.id, (e.currentTarget as HTMLInputElement).value, yearKnown(p.birthday) || !p.birthday)} />
        </label>
        {#if p.birthday}
          <label class="fact small">
            <input type="checkbox" checked={!yearKnown(p.birthday)} onchange={(e) => setBirthday(p.id, yearKnown(p.birthday) ? p.birthday! : `2000-${p.birthday!.slice(2)}`, !(e.currentTarget as HTMLInputElement).checked)} />
            year unknown
          </label>
        {/if}
      </div>
      <button class="icon-btn" aria-label="Remove {p.name}" onclick={() => confirm(`Remove ${p.name}? Their upcoming tasks go too.`) && store.deletePerson(p.id)}><Icon name="trash" size={16} /></button>
    </div>
  {:else}
    <p class="muted">Nobody yet. Find a name above, or add someone by birthday.</p>
  {/each}
  <form class="row" onsubmit={addByBirthday}>
    <input type="text" bind:value={newName} placeholder="Name, e.g. Grandma" aria-label="Name" />
    <input type="date" bind:value={newBirthday} aria-label="Birthday" />
    <button class="btn" type="submit" disabled={!newName.trim() || !newBirthday}>Add birthday</button>
  </form>
</section>

<section class="card">
  <h2>What gets added</h2>
  <p class="help muted">Steps for each kind of occasion. <code>{'{name}'}</code> becomes the person's name. Changes apply to occasions that haven't been added yet.</p>
  {#each kinds as [kind, label] (kind)}
    {@const tpl = store.occasionTemplates.get(kind)}
    <div class="tpl">
      <label class="check">
        <input type="checkbox" checked={tpl?.enabled ?? true} onchange={(e) => store.updateOccasionTemplate(kind, { enabled: (e.currentTarget as HTMLInputElement).checked }).catch(() => {})} />
        <strong>{label}</strong>
      </label>
      {#each stepsOf(kind) as s, i (i)}
        <div class="step">
          <input class="title" type="text" value={s.title} aria-label="Step title" oninput={(e) => edit(kind, i, { title: (e.currentTarget as HTMLInputElement).value })} />
          <label class="days">
            <input type="number" min="0" max="60" value={-s.offset_days} aria-label="Days before" oninput={(e) => edit(kind, i, { offset_days: -Number((e.currentTarget as HTMLInputElement).value) })} />
            <span>{s.offset_days === 0 ? 'on the day' : 'days before'}</span>
          </label>
          <select value={s.task_type_id} aria-label="Task type" onchange={(e) => edit(kind, i, { task_type_id: (e.currentTarget as HTMLSelectElement).value })}>
            {#each types as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
          </select>
          {#if i > 0}
            <label class="small"><input type="checkbox" checked={s.after_previous} onchange={(e) => edit(kind, i, { after_previous: (e.currentTarget as HTMLInputElement).checked })} /> after the previous step</label>
          {/if}
          <button class="icon-btn" aria-label="Remove step" onclick={() => removeStep(kind, i)}><Icon name="x" size={14} /></button>
        </div>
      {/each}
      <div class="row">
        <button class="btn small" onclick={() => addStep(kind)}><Icon name="plus" size={14} /> Step</button>
        {#if drafts[kind]}<button class="btn small primary" onclick={() => save(kind)}>Save</button>{/if}
      </div>
    </div>
  {/each}
</section>

<section class="card">
  <h2>Nameday calendar</h2>
  <p class="help muted">
    {#if store.namedays?.label}{store.namedays.label}: {[...store.namedays.byDate.values()].reduce((n, v) => n + v.length, 0)} names. Shared by everyone on this server.{:else}Not loaded yet.{/if}
  </p>
  {#if store.me?.is_admin}
    <div class="row">
      <button class="btn" disabled={busy} onclick={() => reload()}>{busy ? 'Loading…' : 'Download the official Estonian list'}</button>
    </div>
    <details class="upload">
      <summary>Upload another list instead</summary>
      <p class="help muted">One day per line: <code>15.08 Hanna, Jaana</code> or <code>08-15 Hanna, Jaana</code>. This replaces the current calendar.</p>
      <input type="text" bind:value={uploadLabel} placeholder="Name of the list, e.g. Finnish" />
      <textarea rows="6" bind:value={uploadText} placeholder="01.01 Algo, Alo"></textarea>
      <button class="btn" disabled={busy || !uploadText.trim()} onclick={() => reload({ text: uploadText, label: uploadLabel.trim() || 'Uploaded list' })}>Upload</button>
    </details>
  {/if}
</section>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(480px, 1fr));
    gap: 0 14px;
    align-items: start;
  }
  @media (max-width: 600px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
    margin-bottom: 6px;
  }
  .lead {
    margin: 0 0 14px;
    max-width: 640px;
  }
  section {
    padding: 18px;
    margin-bottom: 14px;
  }
  h2 {
    font-size: 16px;
    margin-bottom: 12px;
  }
  .help {
    font-size: 13px;
    margin: 0 0 12px;
  }
  .small {
    font-size: 12px;
  }
  .search {
    width: 100%;
  }
  .results,
  .upcoming {
    list-style: none;
    padding: 0;
    margin: 8px 0;
  }
  .results li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .upcoming li {
    padding: 3px 0;
    font-size: 14px;
  }
  .when {
    display: inline-block;
    min-width: 120px;
    color: var(--muted);
    font-size: 13px;
  }
  .person {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }
  .person .name {
    max-width: 200px;
    font-weight: 600;
    padding: 4px 8px !important;
  }
  .facts {
    flex: 1;
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    align-items: center;
  }
  .fact {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--muted);
  }
  .fact input[type='date'] {
    width: auto;
    padding: 2px 6px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    margin-top: 10px;
  }
  .row input[type='text'] {
    max-width: 220px;
  }
  .row input[type='date'] {
    width: auto;
  }
  .tpl {
    padding: 8px 0 12px;
    border-bottom: 1px solid var(--border);
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 6px;
  }
  .step {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 8px;
    align-items: center;
    padding: 4px 0;
  }
  .step .title {
    flex: 1 1 220px;
  }
  .days {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--muted);
  }
  .days input {
    width: 64px;
  }
  .step select {
    width: auto;
  }
  .upload {
    margin-top: 12px;
  }
  .upload summary {
    cursor: pointer;
    color: var(--accent);
    font-size: 14px;
    margin-bottom: 8px;
  }
  .upload input,
  .upload textarea {
    width: 100%;
    margin-bottom: 8px;
  }
  code {
    font-size: 12px;
    background: var(--surface-2);
    padding: 1px 5px;
    border-radius: 4px;
  }
</style>
