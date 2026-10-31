<script lang="ts">
  // Who does a shared chore (D-116): anyone in the group, one member, or (for routines and
  // multi-step chores) members taking turns, in the order they're picked.
  import { store } from '../store.svelte'
  import Icon from './Icon.svelte'

  let {
    groupId,
    assignee = $bindable(''),
    rotation = $bindable([]),
    turns = false,
    each = 'time',
    onchange,
  }: {
    groupId: string
    assignee?: string
    rotation?: string[]
    /** Offer taking turns. */
    turns?: boolean
    /** What a turn is: "time", "run"… */
    each?: string
    /** After a change, with the new values (for sheets that save right away). */
    onchange?: (assignee: string, rotation: string[]) => void
  } = $props()

  const group = $derived(store.groups.get(groupId))
  const members = $derived(
    [...(group?.members ?? [])].sort((a, b) => a.display_name.localeCompare(b.display_name)),
  )
  const name = (id: string) => members.find((m) => m.user_id === id)?.display_name ?? 'someone'
  const rotating = $derived(rotation.length > 0)

  function setRotating(on: boolean) {
    rotation = on ? members.map((m) => m.user_id) : []
    onchange?.(assignee, rotation)
  }
  function toggle(id: string) {
    rotation = rotation.includes(id) ? rotation.filter((x) => x !== id) : [...rotation, id]
    onchange?.(assignee, rotation)
  }
</script>

<section>
  <h3><Icon name="users" size={14} /> Who does it</h3>
  {#if !rotating}
    <select bind:value={assignee} onchange={() => onchange?.(assignee, rotation)} aria-label="Who does it">
      <option value="">Anyone in {group?.name ?? 'the group'}</option>
      {#each members as m (m.user_id)}<option value={m.user_id}>{m.user_id === store.me?.id ? `${m.display_name} (you)` : m.display_name}</option>{/each}
    </select>
    <p class="muted help">
      {assignee ? `Everyone sees it, but it's on ${name(assignee)}'s day and lists only. Anyone can still tick it off.` : 'Up for grabs: it shows up for everyone.'}
    </p>
  {/if}
  {#if turns && members.length > 1}
    <label class="check"><input type="checkbox" checked={rotating} onchange={(e) => setRotating(e.currentTarget.checked)} /> Take turns</label>
    {#if rotating}
      <div class="chips" role="group" aria-label="Who takes turns">
        {#each members as m (m.user_id)}
          {@const n = rotation.indexOf(m.user_id)}
          <button class="chip" class:on={n >= 0} aria-pressed={n >= 0} onclick={() => toggle(m.user_id)}>
            {#if n >= 0}<span class="n">{n + 1}</span>{/if}{m.display_name}
          </button>
        {/each}
      </div>
      <p class="muted help">
        One {each} each, in this order: {rotation.map(name).join(' → ') || 'pick who takes part'}. Tap to leave someone out or add them at the end.
      </p>
    {/if}
  {/if}
</section>

<style>
  section {
    margin-top: 18px;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
    margin: 0 0 8px;
  }
  .help {
    font-size: 13px;
    margin: 6px 2px 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    font-size: 14px;
  }
  .chips {
    margin-top: 8px;
  }
  .n {
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    margin-right: 5px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    font-size: 11px;
    font-weight: 700;
  }
</style>
