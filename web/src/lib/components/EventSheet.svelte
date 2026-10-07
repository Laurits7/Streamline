<script lang="ts">
  // A calendar event (read-only from the calendar): its project and the todos for it (D-68).
  import { timeRange } from '../calendar'
  import { longDate, shortDate } from '../dates'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'
  import Check from './Check.svelte'
  import Icon from './Icon.svelte'
  import Sheet from './Sheet.svelte'

  let { id }: { id: string } = $props()

  const ev = $derived(store.events.get(id))
  const date = $derived(ev ? store.eventDate(ev) : '')
  const when = $derived.by(() => {
    if (!ev) return ''
    const day = store.dayEvents(date)
    const d = [...day.allDay, ...day.timed].find((x) => x.event.id === id)
    return d ? timeRange(d) : ''
  })
  const calendar = $derived(ev ? store.calendars.get(ev.calendar_id) : undefined)
  const project = $derived(ev ? store.eventProject(ev) : undefined)
  const todos = $derived(store.eventTasks(id))
  const projects = $derived(store.projectTree())
  const due = $derived(shortDate(date, store.today))

  let title = $state('')
  const close = () => (ui.event = null)

  function add(e: Event) {
    e.preventDefault()
    const t = title.trim()
    if (!t || !ev) return
    store.createTask({ title: t, project_id: project?.id ?? null, event_id: id, due_date: date })
    title = ''
  }
  function open(taskId: string) {
    ui.event = null
    ui.editing = taskId
  }
</script>

{#if ev}
  <Sheet title={ev.title} onclose={close}>
    <div class="when">
      <p>
        <Icon name="calendar" size={14} />
        {shortDate(date, store.today)} · {longDate(date)}{when ? ` · ${when}` : ''}
      </p>
      {#if ev.location}<p><Icon name="pin" size={14} /> {ev.location}</p>{/if}
      <p class="muted">
        {#if calendar}<i class="dot" style:background={calendar.user_color || calendar.color || 'var(--faint)'}></i>{calendar.name}{/if}
        {#if ev.recurring} · <Icon name="repeat" size={12} /> repeats{/if}
        {#if !ev.busy} · free time{/if}
      </p>
    </div>

    <section>
      <h3><Icon name="folder" size={14} /> Project</h3>
      <select
        value={project?.id ?? ''}
        aria-label="Project for this event"
        onchange={(e) => store.setEventProject(ev, (e.currentTarget as HTMLSelectElement).value || null)}>
        <option value="">No project</option>
        {#each projects as { project: p, depth } (p.id)}<option value={p.id}>{'   '.repeat(depth)}{p.name}</option>{/each}
      </select>
      <p class="muted help">
        {ev.recurring ? 'Applies every time this event repeats. ' : ''}The project lists its upcoming events, and new todos
        for the event go into it.
      </p>
    </section>

    <section>
      <h3><Icon name="check" size={14} /> Todos for this event</h3>
      {#if todos.length}
        <ul class="todos">
          {#each todos as t (t.id)}
            <li class:done={t.status !== 'open'}>
              {#if t.status === 'open' || t.status === 'done'}
                <Check done={t.status === 'done'} onclick={() => store.toggleDone(t.id)} label={t.status === 'done' ? `Mark ${t.title} as not done` : `Complete ${t.title}`} />
              {/if}
              <button onclick={() => open(t.id)}>{t.title}</button>
              {#if t.project_id && t.project_id !== project?.id}<span class="muted small">{store.projects.get(t.project_id)?.name}</span>{/if}
            </li>
          {/each}
        </ul>
      {/if}
      <form class="add" onsubmit={add}>
        <input type="text" bind:value={title} maxlength="500" placeholder="Add a todo, e.g. Prepare the agenda" aria-label="New todo for this event" />
        <button class="btn primary" type="submit" disabled={!title.trim()}>Add</button>
      </form>
      <p class="muted help">
        New todos are due {due === 'Today' || due === 'Tomorrow' ? due.toLowerCase() : `on ${due}`}{project ? ` and go into ${project.name}` : ''}.
      </p>
    </section>
  </Sheet>
{/if}

<style>
  .when p {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 4px;
    font-size: 14px;
  }
  .dot {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  section {
    margin-top: 20px;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 650;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin: 0 0 8px;
  }
  select {
    width: 100%;
  }
  .help {
    font-size: 13px;
    margin: 6px 0 0;
  }
  .todos {
    list-style: none;
    padding: 0;
    margin: 0 0 8px;
  }
  .todos li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .todos li button {
    flex: 1;
    text-align: left;
    background: none;
    border: 0;
    padding: 4px 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .todos li.done button {
    text-decoration: line-through;
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .add {
    display: flex;
    gap: 8px;
  }
  .add input {
    flex: 1;
    min-width: 0;
  }
</style>
