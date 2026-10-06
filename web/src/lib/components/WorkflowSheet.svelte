<script lang="ts">
  // Edit a multi-step chore (SPEC §6.3b): an ordered list of steps (each optionally
  // followed by a wait, e.g. while a machine runs) and variants that skip some steps.
  import { untrack } from 'svelte'
  import type { WorkflowStep } from '../api/types/WorkflowStep'
  import type { WorkflowVariant } from '../api/types/WorkflowVariant'
  import { fmtMinutes } from '../dates'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'
  import { ulid } from '../ulid'
  import Icon from './Icon.svelte'
  import Sheet from './Sheet.svelte'

  let { id }: { id: string } = $props()
  const existing = untrack(() => (id === 'new' ? null : (store.workflows.get(id) ?? null)))
  let name = $state(existing?.name ?? '')
  let description = $state(existing?.description ?? '')
  let projectId = $state(existing?.project_id ?? '')
  let steps = $state<WorkflowStep[]>(
    existing ? existing.steps.map((s) => ({ ...s })) : [{ id: ulid(), title: '', estimate_min: null, wait_min: null, difficulty: null }],
  )
  let variants = $state<WorkflowVariant[]>(existing ? existing.variants.map((v) => ({ ...v, skip: [...v.skip] })) : [])
  let saving = $state(false)

  const close = () => (ui.workflow = null)
  const addStep = () => steps.push({ id: ulid(), title: '', estimate_min: null, wait_min: null, difficulty: null })
  function move(i: number, d: number) {
    const j = i + d
    if (j < 0 || j >= steps.length) return
    ;[steps[i], steps[j]] = [steps[j], steps[i]]
  }
  function removeStep(i: number) {
    const sid = steps[i].id
    steps.splice(i, 1)
    for (const v of variants) v.skip = v.skip.filter((x) => x !== sid)
  }
  function toggle(v: WorkflowVariant, sid: string) {
    v.skip = v.skip.includes(sid) ? v.skip.filter((x) => x !== sid) : [...v.skip, sid]
  }

  const valid = $derived(name.trim() && steps.length && steps.every((s) => s.title.trim()) && variants.every((v) => v.name.trim()))

  async function save() {
    saving = true
    const ok = await store.saveWorkflow(existing?.id ?? null, {
      name: name.trim(),
      description,
      project_id: projectId || null,
      steps: steps.map((s) => ({ ...s, title: s.title.trim() })),
      variants: variants.map((v) => ({ ...v, name: v.name.trim() })),
    })
    saving = false
    if (ok) close()
  }
</script>

<Sheet title={existing ? 'Edit multi-step chore' : 'New multi-step chore'} onclose={close}>
  <input class="title-input" type="text" bind:value={name} placeholder="e.g. Laundry" aria-label="Name" />

  <section>
    <h3>Steps, in order</h3>
    <ol class="steps">
      {#each steps as s, i (s.id)}
        <li>
          <span class="n">{i + 1}</span>
          <div class="fields">
            <input type="text" bind:value={s.title} placeholder="Step, e.g. Wash" aria-label="Step {i + 1}" />
            <div class="row">
              <select bind:value={s.estimate_min} aria-label="Estimate">
                <option value={null}>No estimate</option>
                {#each [5, 10, 15, 30, 60] as m (m)}<option value={m}>{fmtMinutes(m)}</option>{/each}
              </select>
              {#if i < steps.length - 1}
                <select bind:value={s.wait_min} aria-label="Wait before the next step">
                  <option value={null}>next step right away</option>
                  {#each [15, 30, 45, 60, 90, 120, 180] as m (m)}<option value={m}>then wait {fmtMinutes(m)}</option>{/each}
                </select>
              {/if}
            </div>
          </div>
          <div class="acts">
            <button class="icon-btn" aria-label="Move up" onclick={() => move(i, -1)} disabled={i === 0}><Icon name="left" size={14} /></button>
            <button class="icon-btn" aria-label="Move down" onclick={() => move(i, 1)} disabled={i === steps.length - 1}><Icon name="right" size={14} /></button>
            <button class="icon-btn" aria-label="Remove step" onclick={() => removeStep(i)} disabled={steps.length === 1}><Icon name="x" size={14} /></button>
          </div>
        </li>
      {/each}
    </ol>
    <button class="btn small" onclick={addStep}><Icon name="plus" size={14} /> Add step</button>
  </section>

  <section>
    <h3>Variants (optional)</h3>
    <p class="muted help">For different kinds of loads: untick the steps a variant leaves out (e.g. delicates: no dryer).</p>
    {#each variants as v, vi (v.id)}
      <div class="variant">
        <div class="row">
          <input type="text" bind:value={v.name} placeholder="e.g. Delicates" aria-label="Variant name" />
          <button class="icon-btn" aria-label="Remove variant" onclick={() => variants.splice(vi, 1)}><Icon name="x" size={14} /></button>
        </div>
        <div class="chips">
          {#each steps as s (s.id)}
            <button class="chip" class:on={!v.skip.includes(s.id)} aria-pressed={!v.skip.includes(s.id)} onclick={() => toggle(v, s.id)}>{s.title || '…'}</button>
          {/each}
        </div>
      </div>
    {/each}
    <button class="btn small" onclick={() => variants.push({ id: ulid(), name: '', skip: [] })}><Icon name="plus" size={14} /> Add variant</button>
  </section>

  <section>
    <h3>Project for the steps</h3>
    <select bind:value={projectId}>
      <option value="">Inbox</option>
      {#each store.projectTree() as { project: p, depth } (p.id)}<option value={p.id}>{'   '.repeat(depth)}{p.name}</option>{/each}
    </select>
  </section>

  <section>
    <h3>Notes</h3>
    <textarea bind:value={description} rows="2" placeholder="Shown on the first step"></textarea>
  </section>

  <footer>
    {#if existing}
      <button class="btn danger" onclick={() => { if (confirm(`Delete “${existing.name}”? Chores already started stay.`)) { store.deleteWorkflow(existing.id); close() } }}>
        <Icon name="trash" size={16} /> Delete
      </button>
    {:else}<span></span>{/if}
    <button class="btn primary" disabled={!valid || saving} onclick={save}>{existing ? 'Save' : 'Create'}</button>
  </footer>
</Sheet>

<style>
  .title-input {
    font-size: 19px !important;
    font-weight: 600;
  }
  section {
    margin-top: 18px;
  }
  h3 {
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
    margin-bottom: 8px;
  }
  .steps {
    list-style: none;
    padding: 0;
    margin: 0 0 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .steps li {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
  .n {
    flex: none;
    width: 24px;
    height: 24px;
    margin-top: 8px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
    display: grid;
    place-items: center;
    font-size: 12px;
    font-weight: 700;
  }
  .fields {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .row select {
    width: auto;
    padding: 4px 8px;
    font-size: 13px;
  }
  .acts {
    display: flex;
    flex-direction: column;
  }
  .acts .icon-btn {
    width: 28px;
    height: 24px;
  }
  .acts .icon-btn:nth-child(1) :global(svg) {
    transform: rotate(90deg);
  }
  .acts .icon-btn:nth-child(2) :global(svg) {
    transform: rotate(90deg);
  }
  .variant {
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 8px;
    margin-bottom: 8px;
  }
  .variant .chips {
    margin-top: 6px;
  }
  .help {
    font-size: 12px;
    margin: -4px 0 8px;
  }
  footer {
    display: flex;
    justify-content: space-between;
    margin-top: 24px;
  }
</style>
