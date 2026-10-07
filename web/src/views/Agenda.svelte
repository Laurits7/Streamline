<script lang="ts">
  // Week agenda (SPEC §6.5): calendar events and timed tasks, day by day.
  import { timeRange } from '../lib/calendar'
  import Check from '../lib/components/Check.svelte'
  import Icon from '../lib/components/Icon.svelte'
  import { addDays, dayLabel, longDate } from '../lib/dates'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'
  import { ui } from '../lib/ui.svelte'

  let { start: startProp = null }: { start?: string | null } = $props()

  /** First day of the week containing `date` (week_start: 1 = Monday … 7 = Sunday). */
  function weekOf(date: string) {
    const iso = ((new Date(date + 'T00:00:00Z').getUTCDay() + 6) % 7) + 1
    return addDays(date, -((iso + 7 - (store.me?.week_start ?? 1)) % 7))
  }
  const start = $derived(weekOf(startProp ?? store.today))
  const days = $derived(Array.from({ length: 7 }, (_, i) => addDays(start, i)))

  type Row =
    | { kind: 'event'; key: string; at: number; label: string; title: string; sub: string | null; color: string; todos: number; open: number }
    | { kind: 'task'; key: string; at: number; label: string; title: string; taskId: string; done: boolean }

  function agenda(date: string) {
    const ev = store.dayEvents(date)
    const rows: Row[] = ev.timed.map((d) => ({
      kind: 'event',
      key: d.event.id,
      at: d.start,
      label: timeRange(d),
      title: d.event.title,
      sub: [d.event.location, store.eventProject(d.event)?.name].filter(Boolean).join(' · ') || null,
      color: d.color,
      todos: store.eventTasks(d.event.id).length,
      open: store.eventTasks(d.event.id).filter((t) => t.status === 'open').length,
    }))
    let untimed = 0
    for (const e of store.dayEntries(date)) {
      const t = store.tasks.get(e.task_id)
      if (!t || t.status === 'missed' || t.status === 'skipped') continue
      if (!e.start_time) {
        if (t.status === 'open') untimed++
        continue
      }
      const at = Number(e.start_time.slice(0, 2)) * 60 + Number(e.start_time.slice(3, 5))
      rows.push({ kind: 'task', key: e.id, at, label: e.start_time, title: t.title, taskId: t.id, done: t.status === 'done' })
    }
    rows.sort((a, b) => a.at - b.at)
    return { allDay: ev.allDay, rows, untimed }
  }

  $effect(() => {
    store.loadNamedays().catch(() => {})
  })

  const go = (d: string | null) => router.go(d ? `/agenda/${d}` : '/agenda')
</script>

<header class="head">
  <h1>Agenda</h1>
  <nav class="weeknav" aria-label="Change week">
    <button class="icon-btn" onclick={() => go(addDays(start, -7))} aria-label="Previous week"><Icon name="left" /></button>
    {#if start !== weekOf(store.today)}<button class="btn small" onclick={() => go(null)}>This week</button>{/if}
    <button class="icon-btn" onclick={() => go(addDays(start, 7))} aria-label="Next week"><Icon name="right" /></button>
  </nav>
</header>
{#if !store.calendarAccount}
  <p class="muted hint">
    Connect your calendar in <a href="/settings">Settings</a> to see its events here.
  </p>
{/if}

<div class="week">
  {#each days as date (date)}
    {@const a = agenda(date)}
    <section class="day card" class:today={date === store.today} class:past={date < store.today}>
      <a class="dayhead" href={date === store.today ? '/' : `/day/${date}`}>
        <strong>{dayLabel(date, store.today)}</strong>
        <span class="muted">{longDate(date)}</span>
      </a>
      {#each store.occasionsOn(date) as o (o.person.id + o.kind)}
        <div class="occasion"><Icon name="gift" size={13} /> {o.person.name}: {o.kind === 'birthday' ? 'birthday' : 'nameday'}</div>
      {/each}
      {#each a.allDay as d (d.event.id)}
        <button class="allday" style:--cal={d.color} onclick={() => (ui.event = d.event.id)}>{d.event.title}</button>
      {/each}
      {#each a.rows as r (r.key)}
        {#if r.kind === 'event'}
          <div class="row" style:--cal={r.color}>
            <span class="time">{r.label}</span>
            <span class="dot" aria-hidden="true"></span>
            <button class="what" onclick={() => (ui.event = r.key)}>
              {r.title}{#if r.sub}<span class="muted">{` · ${r.sub}`}</span>{/if}{#if r.todos}<span class="muted">{` · ✓ ${r.todos - r.open}/${r.todos}`}</span>{/if}
            </button>
          </div>
        {:else}
          <div class="row task" class:done={r.done}>
            <span class="time">{r.label}</span>
            <Check done={r.done} onclick={() => store.toggleDone(r.taskId)} label="Complete {r.title}" />
            <button class="what" onclick={() => (ui.editing = r.taskId)}>{r.title}</button>
          </div>
        {/if}
      {/each}
      {#if a.untimed}
        <a class="more muted" href={date === store.today ? '/' : `/day/${date}`}>+ {a.untimed} planned without a time</a>
      {/if}
      {#if !a.allDay.length && !a.rows.length && !a.untimed}<p class="muted none">Nothing scheduled</p>{/if}
      {#if store.namedaysOn(date).length}<p class="muted nd">Nameday: {store.namedaysOn(date).join(', ')}</p>{/if}
    </section>
  {/each}
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .weeknav {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .hint {
    font-size: 14px;
    margin: 0 0 12px;
  }
  .week {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  .day {
    padding: 12px 14px;
  }
  .day.today {
    outline: 2px solid var(--accent);
  }
  .day.past {
    background: var(--bg);
    box-shadow: none;
  }
  .dayhead {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 8px;
    color: inherit;
  }
  .dayhead span {
    font-size: 13px;
  }
  .occasion {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 13px;
    font-weight: 600;
    color: var(--accent);
    margin-bottom: 4px;
  }
  .nd {
    font-size: 12px;
    margin: 6px 0 0;
  }
  .allday {
    display: block;
    width: 100%;
    text-align: left;
    font-size: 13px;
    font-weight: 600;
    padding: 3px 8px;
    margin-bottom: 4px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--cal) 16%, var(--surface));
    border-left: 3px solid var(--cal);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    font-size: 14px;
  }
  .time {
    width: 92px;
    flex: none;
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 10px;
    height: 10px;
    flex: none;
    border-radius: 50%;
    background: var(--cal);
    margin: 0 4px;
  }
  .what {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .task :global(.check) {
    width: 18px;
    height: 18px;
  }
  .task.done .what {
    text-decoration: line-through;
    color: var(--muted);
  }
  .more {
    display: block;
    font-size: 13px;
    margin-top: 4px;
  }
  .none {
    font-size: 13px;
    margin: 0;
  }
</style>
