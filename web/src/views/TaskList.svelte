<script lang="ts">
  // Inbox (projectId = null) or a single project's todo list.
  import Icon from '../lib/components/Icon.svelte'
  import QuickAdd from '../lib/components/QuickAdd.svelte'
  import TaskRow from '../lib/components/TaskRow.svelte'
  import TaskViews from '../lib/components/TaskViews.svelte'
  import { router } from '../lib/router.svelte'
  import { droppable, dropList, type DragItem } from '../lib/dnd.svelte'
  import { keyAt } from '../lib/order'
  import { store } from '../lib/store.svelte'
  import { ui } from '../lib/ui.svelte'
  import { localTime } from '../lib/calendar'
  import { shortDate } from '../lib/dates'
  import { linkify } from '../lib/text'
  import { toast } from '../lib/toast.svelte'

  let { projectId = null }: { projectId?: string | null } = $props()

  const project = $derived(projectId ? store.projects.get(projectId) : null)
  const open = $derived(store.tasksIn(projectId))
  const closed = $derived(store.tasksIn(projectId, 'closed'))
  const crumbs = $derived(projectId ? store.ancestors(projectId) : [])
  const children = $derived(projectId ? store.childProjects(projectId) : [])
  const upcoming = $derived(projectId ? store.projectEvents(projectId) : [])
  // Valid new parents for "Move under…": anything outside this project's own subtree.
  const parentChoices = $derived(
    projectId ? store.projectTree().filter(({ project: p }) => store.canMoveProject(projectId, p.id)) : [],
  )
  let showDone = $state(false)
  let menu = $state(false)
  let editingAbout = $state(false)
  let aboutDraft = $state('')

  function editAbout() {
    aboutDraft = project?.description ?? ''
    editingAbout = true
  }
  function saveAbout(e: Event) {
    e.preventDefault()
    if (projectId) store.updateProject(projectId, { description: aboutDraft.trimEnd() })
    editingAbout = false
  }
  /** Ideas ask for no attention until activated (D-72); a project can go back to being one. */
  function setIdea(idea: boolean) {
    if (!projectId) return
    const ids = new Set(store.subtree(projectId))
    const planned = idea
      ? [...store.entries.values()].filter((e) => {
          const t = store.tasks.get(e.task_id)
          return e.date >= store.today && t?.status === 'open' && !!t.project_id && ids.has(t.project_id)
        }).length
      : 0
    store.updateProject(projectId, { status: idea ? 'idea' : 'active' })
    menu = false
    if (idea && planned) toast(`Moved to ideas. ${planned} planned task${planned === 1 ? ' was' : 's were'} taken off your plan.`)
    else if (!idea) toast(`${project?.name} is active. Its tasks can be planned now.`)
  }

  function rename() {
    const name = prompt('Project name', project?.name)?.trim()
    if (name && projectId) store.updateProject(projectId, { name })
    menu = false
  }
  function archive() {
    if (!projectId || !project) return
    store.updateProject(projectId, { archived: !project.archived_at })
    menu = false
  }
  function moveUnder(parentId: string | null) {
    if (projectId) store.moveProject(projectId, parentId)
    menu = false
  }
  function remove() {
    if (!projectId || !project) return
    const ids = new Set(store.subtree(projectId))
    const tasks = [...store.tasks.values()].filter((t) => t.project_id && ids.has(t.project_id)).length
    const subs = ids.size - 1
    const what = `${tasks} task${tasks === 1 ? '' : 's'}${subs ? ` and ${subs} subproject${subs === 1 ? '' : 's'}` : ''}`
    if (confirm(`Delete “${project.name}” with its ${what}?`)) {
      store.deleteProject(projectId)
      router.go('/projects', true)
    }
  }
  /** Drop a task into a project's list: reorder, or move it there from elsewhere. */
  function dropInto(target: string | null, list: { id: string; position: string }[], item: DragItem, index: number) {
    if (item.kind !== 'task') return
    const others = list.filter((t) => t.id !== item.taskId).map((t) => t.position)
    store.moveToProject(item.taskId, target, keyAt(others, index))
  }
  const colors = ['#4f46e5', '#0891b2', '#16a34a', '#ca8a04', '#ea580c', '#dc2626', '#db2777', '#7c3aed', '#64748b']
</script>

{#if projectId && !project}
  <p class="empty">{store.ready ? 'Project not found.' : ''}</p>
{:else}
  <header class="head">
    <div>
      {#if crumbs.length}
        <nav class="crumbs" aria-label="Parent projects">
          {#each crumbs as c (c.id)}<a href="/projects/{c.id}">{c.name}</a><span aria-hidden="true">›</span>{/each}
        </nav>
      {/if}
      <h1>
        {#if project}<i class="dot" style:background={project.color ?? 'var(--faint)'}></i>{project.name}{#if project.status === 'idea'}<span class="idea-tag big">Idea</span>{/if}{:else}Inbox{/if}
        {#if project?.owner_group_id}<span class="shared" title="Shared with {store.groupName(project.owner_group_id)}"><Icon name="users" size={16} /> {store.groupName(project.owner_group_id)}</span>{/if}
      </h1>
      <p class="muted">
        {[
          `${open.length} open`,
          children.length ? `${children.length} subproject${children.length === 1 ? '' : 's'}` : '',
          project?.archived_at ? 'archived' : '',
        ]
          .filter(Boolean)
          .join(' · ')}
      </p>
    </div>
    {#if project}
      <div class="menu-wrap">
        <button class="icon-btn" onclick={() => (menu = !menu)} aria-label="Project actions" aria-expanded={menu}><Icon name="more" /></button>
        {#if menu}
          <div class="menu card" role="menu">
            <button role="menuitem" onclick={rename}><Icon name="edit" size={16} /> Rename</button>
            <div class="colors">
              {#each colors as c (c)}
                <button class="swatch" style:background={c} aria-label="Color {c}" onclick={() => { store.updateProject(projectId!, { color: c }); menu = false }}></button>
              {/each}
            </div>
            {#if project.parent_id}
              <button role="menuitem" onclick={() => moveUnder(null)}><Icon name="folder" size={16} /> Make top-level project</button>
            {/if}
            <label class="move">
              <span>Move under</span>
              <select
                value={project.parent_id ?? ''}
                onchange={(e) => moveUnder((e.currentTarget as HTMLSelectElement).value || null)}>
                <option value="">— Top level —</option>
                {#each parentChoices as { project: p, depth } (p.id)}
                  <option value={p.id}>{'\u00a0\u00a0'.repeat(depth)}{p.name}</option>
                {/each}
              </select>
            </label>
            {#if store.myGroups().length && !project.parent_id}
              <label class="move">
                <span>Shared with</span>
                <select
                  value={project.owner_group_id ?? ''}
                  onchange={(e) => {
                    store.shareProject(projectId!, (e.currentTarget as HTMLSelectElement).value || null)
                    menu = false
                  }}>
                  <option value="">Just me</option>
                  {#each store.myGroups() as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
                </select>
              </label>
            {/if}
            {#if store.places.size}
              <label class="move">
                <span>Default place for new tasks</span>
                <select
                  value={project.default_place_id ?? ''}
                  onchange={(e) => {
                    store.updateProject(projectId!, { default_place_id: (e.currentTarget as HTMLSelectElement).value || null })
                    menu = false
                  }}>
                  <option value="">Anywhere</option>
                  {#each store.placeList() as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
                </select>
              </label>
            {/if}
            <button role="menuitem" onclick={() => setIdea(project.status !== 'idea')}>
              <Icon name="bulb" size={16} /> {project.status === 'idea' ? 'Activate' : 'Move to ideas'}
            </button>
            <button role="menuitem" onclick={archive}><Icon name="archive" size={16} /> {project.archived_at ? 'Unarchive' : 'Archive'}</button>
            <button role="menuitem" class="danger" onclick={remove}><Icon name="trash" size={16} /> Delete</button>
          </div>
        {/if}
      </div>
    {/if}
  </header>

  {#if project}
    {#if project.status === 'idea'}
      <div class="idea-note">
        <Icon name="bulb" size={18} />
        <p>
          {store.ideaRoot(project) ? 'This is an idea' : `Part of an idea`}: write things down, but its tasks stay out of your day, plans
          and reminders until you activate it.
        </p>
        <button class="btn primary small" onclick={() => setIdea(false)}>Activate</button>
      </div>
    {/if}
    <section class="about" aria-label="Description">
      {#if editingAbout}
        <form onsubmit={saveAbout}>
          <!-- svelte-ignore a11y_autofocus -->
          <textarea
            bind:value={aboutDraft}
            rows="5"
            maxlength="10000"
            autofocus
            aria-label="Project description"
            placeholder="What is this project about? Goals, notes, links…"></textarea>
          <div class="about-actions">
            <button class="btn primary small" type="submit">Save</button>
            <button class="btn small" type="button" onclick={() => (editingAbout = false)}>Cancel</button>
          </div>
        </form>
      {:else if project.description}
        <p class="desc">
          {#each linkify(project.description) as part, i (i)}{#if part.url}<a href={part.url} target="_blank" rel="noopener noreferrer">{part.text}</a>{:else}{part.text}{/if}{/each}
        </p>
        <button class="about-edit" onclick={editAbout}><Icon name="edit" size={13} /> Edit description</button>
      {:else}
        <button class="about-edit" onclick={editAbout}><Icon name="plus" size={14} /> Add a description</button>
      {/if}
    </section>
  {/if}

  {#if upcoming.length}
    <section class="events" aria-label="Upcoming events">
      <h2><Icon name="calendar" size={14} /> Upcoming events</h2>
      <ul>
        {#each upcoming as ev (ev.id)}
          {@const todos = store.eventTasks(ev.id)}
          <li>
            <button onclick={() => (ui.event = ev.id)}>
              <span class="when">{shortDate(store.eventDate(ev), store.today)}{ev.all_day ? '' : ` ${localTime(ev.start_at!, store.me?.timezone ?? 'UTC')}`}</span>
              <span class="what">{ev.title}</span>
              {#if todos.length}<span class="muted">✓ {todos.filter((t) => t.status !== 'open').length}/{todos.length}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <TaskViews scope={projectId ? { kind: 'project', id: projectId } : { kind: 'inbox' }}>
    {#snippet list()}
    <QuickAdd placeholder={project ? `Add to ${project.name}…` : 'Add to inbox…'} onadd={(title) => store.createTask({ title, project_id: projectId })} />

    <div
      class="card list"
      use:dropList={{
        accepts: (it) => it.kind === 'task',
        drop: (it, i) => dropInto(projectId, open, it, i),
        keyMove: (id, index) => store.reorderTask(open, id, index),
      }}>
      {#each open as t (t.id)}
        <TaskRow task={t} handle planButton />
      {:else}
        <p class="empty">{project ? 'No open tasks in this project.' : 'Inbox zero. Capture anything above.'}</p>
      {/each}
    </div>

    {#if project}
      <h2 class="section-title"><Icon name="folder" size={14} /> Subprojects</h2>
      {#each children as c (c.id)}
        {@const tasks = store.tasksIn(c.id)}
        {@const subs = store.childProjects(c.id).length}
        <div class="card sub">
          <a
            class="sub-head drop-zone"
            href="/projects/{c.id}"
            draggable="false"
            use:droppable={{ accepts: (it) => it.kind === 'task', drop: (it) => it.kind === 'task' && store.moveToProject(it.taskId, c.id) }}>
            <i class="dot small" style:background={c.color ?? 'var(--faint)'}></i>
            <span class="sub-name">{c.name}</span>
            {#if subs}<span class="muted small">{subs} sub</span>{/if}
            <span class="muted small">{store.openCountDeep(c.id)} open</span>
            <Icon name="right" size={16} />
          </a>
          <div
            class="list"
            use:dropList={{
              accepts: (it) => it.kind === 'task',
              drop: (it, i) => dropInto(c.id, tasks, it, i),
              keyMove: (id, index) => store.reorderTask(tasks, id, index),
            }}>
            {#each tasks as t (t.id)}
              <TaskRow task={t} handle planButton />
            {:else}
              <p class="empty small">No open tasks here. Drop tasks to move them in.</p>
            {/each}
          </div>
        </div>
      {/each}
      <QuickAdd placeholder="Add a subproject…" onadd={(name) => store.createProject(name, projectId)} />
    {/if}

    {#if closed.length}
      <button class="section-title toggle" onclick={() => (showDone = !showDone)} aria-expanded={showDone}>
        <Icon name={showDone ? 'left' : 'right'} size={14} /> Completed ({closed.length})
      </button>
      {#if showDone}
        <div class="card list">
          {#each closed.slice(0, 100) as t (t.id)}
            <TaskRow task={t} />
          {/each}
        </div>
      {/if}
    {/if}
    {/snippet}
  </TaskViews>
{/if}

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 16px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .shared {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-soft);
    border-radius: 999px;
    padding: 2px 10px;
  }
  .crumbs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    font-size: 13px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .crumbs a:hover {
    color: var(--accent);
  }
  .sub {
    overflow: hidden;
    margin-bottom: 12px;
  }
  .sub .list {
    margin-top: 0;
    border-top: 1px solid var(--border);
    border-radius: 0;
  }
  .sub-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    font-weight: 600;
  }
  .sub-head:hover {
    background: var(--surface-2);
  }
  .sub-name {
    flex: 1;
  }
  .small {
    font-size: 12px;
    font-weight: 500;
  }
  .empty.small {
    padding: 14px;
    margin: 0;
  }
  .dot.small {
    width: 10px;
    height: 10px;
  }
  .move {
    display: block;
    padding: 6px 10px;
  }
  .move span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .move select {
    padding: 6px 8px;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }
  .head p {
    margin: 2px 0 0;
  }
  .list {
    overflow: hidden;
    margin-top: 12px;
  }
  .toggle {
    width: 100%;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: 40px;
    z-index: 20;
    min-width: 200px;
    padding: 6px;
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
  }
  .menu > button {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 10px;
    border-radius: 8px;
    text-align: left;
  }
  .menu > button:hover {
    background: var(--surface-2);
  }
  .menu .danger {
    color: var(--danger);
  }
  .colors {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 6px 10px;
  }
  .swatch {
    width: 20px;
    height: 20px;
    border-radius: 50%;
  }
  .events {
    margin: 0 0 16px;
  }
  .events h2 {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 650;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin: 0 0 6px;
  }
  .events ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .events button {
    display: flex;
    gap: 8px;
    align-items: baseline;
    padding: 6px 12px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    font-size: 14px;
    max-width: 100%;
  }
  .events .when {
    font-weight: 600;
    white-space: nowrap;
  }
  .events .what {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .idea-tag.big {
    font-size: 12px;
    padding: 4px 8px;
    margin-left: 10px;
    vertical-align: middle;
  }
  .idea-note {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    margin-bottom: 14px;
    border: 1px dashed var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--muted);
  }
  .idea-note p {
    flex: 1;
    margin: 0;
    font-size: 14px;
  }
  .about {
    margin-bottom: 18px;
  }
  .desc {
    margin: 0 0 4px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-width: 70ch;
    line-height: 1.55;
  }
  .desc a {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .about textarea {
    width: 100%;
    resize: vertical;
  }
  .about-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .about-edit {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: 0;
    padding: 4px 0;
    font: inherit;
    font-size: 13px;
    color: var(--muted);
    cursor: pointer;
  }
  .about-edit:hover {
    color: var(--accent);
  }
  @media (max-width: 520px) {
    .idea-note {
      flex-wrap: wrap;
    }
  }
</style>
