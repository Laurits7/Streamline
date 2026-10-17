<script lang="ts">
  // The goals a task, project or routine counts towards (D-112). The links live on the
  // goals, so adding or removing one changes the goal.
  import { goalTree, type LinkKind } from '../goals'
  import { store } from '../store.svelte'
  import Icon from './Icon.svelte'

  let { kind, id }: { kind: LinkKind; id: string } = $props()
  const linked = $derived(store.goalsLinking(kind, id))
  // Outcome goals that are still open (habits and rules don't count linked work).
  const options = $derived(
    goalTree(store.goalList().filter((g) => g.kind === 'outcome' && (g.status === 'active' || g.status === 'paused'))).filter(
      ({ goal }) => !linked.some((l) => l.id === goal.id),
    ),
  )
</script>

<div class="goal-links">
  {#each linked as g (g.id)}
    <span class="chip on link-chip">
      <Icon name="flag" size={12} />
      <a href="/goals">{g.title}</a>
      <button aria-label="Stop counting towards {g.title}" onclick={() => store.unlinkFromGoal(g.id, kind, id)}><Icon name="x" size={12} /></button>
    </span>
  {/each}
  {#if options.length}
    <select
      class="add-goal"
      value=""
      aria-label="Count towards a goal"
      onchange={(e) => {
        const v = (e.currentTarget as HTMLSelectElement).value
        if (v) store.linkToGoal(v, kind, id)
        ;(e.currentTarget as HTMLSelectElement).value = ''
      }}>
      <option value="">+ Add to a goal</option>
      {#each options as { goal, depth } (goal.id)}<option value={goal.id}>{'   '.repeat(depth)}{goal.title}</option>{/each}
    </select>
  {:else if !linked.length}
    <span class="muted">No open goals yet. <a href="/goals">Add one</a>.</span>
  {/if}
</div>

<style>
  .goal-links {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .link-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding-right: 6px;
  }
  .link-chip a {
    color: inherit;
    text-decoration: none;
  }
  .link-chip button {
    display: grid;
    color: inherit;
  }
  .add-goal {
    width: auto !important;
    padding: 4px 8px !important;
    font-size: 13px;
  }
</style>
