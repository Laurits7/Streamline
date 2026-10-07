<script lang="ts">
  // Settings → Default tasks (D-71): ready-made tasks with a checklist, added in two taps
  // from any "Add a task…" field.
  import type { TaskTemplate } from '../api/types/TaskTemplate'
  import { fmtMinutes } from '../dates'
  import { store } from '../store.svelte'
  import Checklist from './Checklist.svelte'
  import Icon from './Icon.svelte'

  let open = $state<string | null>(null)
  let newTitle = $state('')
  const projects = $derived(store.projectTree())

  const save = (t: TaskTemplate, patch: Partial<TaskTemplate>) => store.saveDefaultTask(t.id, patch)
  async function create(e: Event) {
    e.preventDefault()
    const title = newTitle.trim()
    if (!title) return
    const t = await store.saveDefaultTask(null, { title })
    if (t) {
      newTitle = ''
      open = t.id
    }
  }
</script>

{#each store.defaultTasks() as t (t.id)}
  <div class="group card">
    <div class="line top">
      <button class="name" aria-expanded={open === t.id} onclick={() => (open = open === t.id ? null : t.id)}>
        <Icon name={open === t.id ? 'left' : 'right'} size={14} />
        <strong>{t.title}</strong>
        <span class="muted small">
          {t.checklist.length ? `${t.checklist.length} step${t.checklist.length === 1 ? '' : 's'}` : 'no steps'}{t.estimate_min ? ` · ${fmtMinutes(t.estimate_min)}` : ''}{t.owner_group_id ? ` · ${store.groupName(t.owner_group_id)}` : ''}
        </span>
      </button>
      <button class="btn small danger" onclick={() => confirm(`Delete the default task “${t.title}”? Tasks made from it stay.`) && store.deleteDefaultTask(t.id)}>Delete</button>
    </div>
    {#if open === t.id}
      <div class="edit">
        <label>
          <span>Title</span>
          <input type="text" value={t.title} maxlength="500" onchange={(e) => save(t, { title: (e.currentTarget as HTMLInputElement).value })} />
        </label>
        <div>
          <span class="lbl">Checklist</span>
          <Checklist items={t.checklist} onchange={(checklist) => save(t, { checklist })} tickable={false} />
        </div>
        <div class="grid">
          <label>
            <span>Estimate</span>
            <select value={t.estimate_min ?? ''} onchange={(e) => { const v = (e.currentTarget as HTMLSelectElement).value; save(t, { estimate_min: v ? Number(v) : null }) }}>
              <option value="">None</option>
              {#each [5, 10, 15, 20, 30, 45, 60, 90, 120] as m (m)}<option value={m}>{fmtMinutes(m)}</option>{/each}
            </select>
          </label>
          <label>
            <span>Project</span>
            <select value={t.project_id ?? ''} onchange={(e) => save(t, { project_id: (e.currentTarget as HTMLSelectElement).value || null })}>
              <option value="">Where it's added</option>
              {#each projects as p (p.project.id)}<option value={p.project.id}>{'  '.repeat(p.depth)}{p.project.name}</option>{/each}
            </select>
          </label>
          {#if store.myGroups().length}
            <label>
              <span>Shared with</span>
              <select value={t.owner_group_id ?? ''} onchange={(e) => save(t, { owner_group_id: (e.currentTarget as HTMLSelectElement).value || null })}>
                <option value="">Just me</option>
                {#each store.myGroups() as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
              </select>
            </label>
          {/if}
        </div>
        <label>
          <span>Notes</span>
          <textarea rows="2" value={t.notes} onchange={(e) => save(t, { notes: (e.currentTarget as HTMLTextAreaElement).value })}></textarea>
        </label>
      </div>
    {/if}
  </div>
{/each}
<form class="row" onsubmit={create}>
  <input type="text" bind:value={newTitle} placeholder="New default task, e.g. Do laundry" />
  <button class="btn" type="submit">Create</button>
</form>

<style>
  .group {
    padding: 10px 12px;
    margin-bottom: 10px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 10px;
    justify-content: space-between;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    text-align: left;
    flex-wrap: wrap;
  }
  .small {
    font-size: 12px;
  }
  .edit {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 10px;
  }
  label,
  .grid > label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  label span,
  .lbl {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 10px;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .row input {
    flex: 1;
  }
</style>
