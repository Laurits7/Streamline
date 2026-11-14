<script lang="ts">
  // Start a multi-step chore: pick the variants (one chain each, e.g. two loads) and, for a
  // household one, who does it (D-116).
  import { untrack } from 'svelte'
  import { assigneeFor } from '../assign'
  import { store } from '../store.svelte'
  import { toast } from '../toast.svelte'
  import { ui } from '../ui.svelte'
  import AssignPicker from './AssignPicker.svelte'
  import Icon from './Icon.svelte'
  import Sheet from './Sheet.svelte'

  let { id }: { id: string } = $props()
  const w = untrack(() => store.workflows.get(id))
  let chosen = $state<string[]>([])
  let planToday = $state(true)
  let busy = $state(false)
  const close = () => (ui.startWorkflow = null)
  // Whose turn it is (or the chore's default); picking someone else sends them instead.
  const members = untrack(() => (w?.owner_group_id ? (store.groups.get(w.owner_group_id)?.members ?? []).map((m) => m.user_id) : []))
  const due = w ? assigneeFor(w.assignee_user_id, w.rotation_user_ids, members, w.rotation_turn) : null
  let assignee = $state(due ?? '')
  const picked = $derived((assignee || null) !== due)

  const included = (skip: string[]) => (w?.steps ?? []).filter((s) => !skip.includes(s.id)).map((s) => s.title).join(' → ')

  async function start() {
    if (!w) return
    busy = true
    const tasks = await store.startWorkflow(w.id, chosen, planToday ? store.today : null, picked ? assignee || null : undefined)
    busy = false
    if (tasks.length) {
      const runs = new Set(tasks.map((t) => t.workflow_instance_id)).size
      toast(`Started ${w.name}${runs > 1 ? ` (${runs} runs)` : ''}`)
      close()
    }
  }
</script>

{#if w}
  <Sheet title="Start {w.name}" onclose={close}>
    {#if w.variants.length}
      <p class="muted">Which ones? Each gets its own chain of steps.</p>
      <div class="options">
        {#each w.variants as v (v.id)}
          <button
            class="option"
            class:on={chosen.includes(v.id)}
            aria-pressed={chosen.includes(v.id)}
            onclick={() => (chosen = chosen.includes(v.id) ? chosen.filter((x) => x !== v.id) : [...chosen, v.id])}>
            <strong>{v.name}</strong>
            <span>{included(v.skip)}</span>
          </button>
        {/each}
      </div>
    {:else}
      <p class="muted">{included([])}</p>
    {/if}
    {#if w.owner_group_id}
      <AssignPicker groupId={w.owner_group_id} bind:assignee />
      {#if w.rotation_user_ids.length && due}<p class="muted turn">It's {store.personName(due)}'s turn.</p>{/if}
    {/if}
    <label class="check"><input type="checkbox" bind:checked={planToday} /> Put the first step on {assignee && assignee !== store.me?.id ? `${store.personName(assignee)}'s` : 'today’s'} plan</label>
    <button class="btn primary big" disabled={busy || (w.variants.length > 0 && chosen.length === 0)} onclick={start}>
      <Icon name="play" size={16} /> Start{chosen.length > 1 ? ` ${chosen.length} runs` : ''}
    </button>
  </Sheet>
{/if}

<style>
  .options {
    display: grid;
    gap: 6px;
    margin: 10px 0;
  }
  .option {
    display: flex;
    flex-direction: column;
    text-align: left;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
  }
  .option span {
    font-size: 12px;
    color: var(--muted);
  }
  .option.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 14px 0;
  }
  .turn {
    font-size: 13px;
    margin: 6px 2px 0;
  }
  .big {
    width: 100%;
    min-height: 46px;
  }
</style>
