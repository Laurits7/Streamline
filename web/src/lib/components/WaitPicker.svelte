<script lang="ts">
  // "Wait for results…" (D-70): when to check back, and what's being waited for.
  import { untrack } from 'svelte'
  import { store } from '../store.svelte'
  import { checkBackChoices, localParts, zonedToUtc } from '../waiting'

  let { taskId, ondone }: { taskId: string; ondone?: () => void } = $props()
  const task = $derived(store.tasks.get(taskId))
  const tz = $derived(store.me?.timezone ?? 'UTC')
  const choices = $derived(checkBackChoices(Date.now(), tz, store.me?.day_window_start ?? '08:00'))
  let note = $state('')
  $effect.pre(() => {
    const id = taskId
    note = untrack(() => store.tasks.get(id)?.waiting_note ?? '')
  })
  let custom = $state('')
  const min = $derived.by(() => {
    const p = localParts(new Date().toISOString(), tz)
    return `${p.date}T${p.time}`
  })

  function pick(at: string | null) {
    store.waitFor(taskId, at, note)
    ondone?.()
  }
  function pickCustom() {
    if (!custom) return
    pick(zonedToUtc(custom.slice(0, 10), custom.slice(11, 16), tz))
  }
</script>

{#if task}
  <div class="wait-picker">
    <label class="note">
      <span>Waiting for</span>
      <input type="text" bind:value={note} maxlength="500" placeholder="e.g. the training run, a reply from the bank" />
    </label>
    <span class="label">Check back</span>
    <div class="chips">
      {#each choices as c (c.label)}
        <button class="chip" onclick={() => pick(c.at)}>{c.label}</button>
      {/each}
    </div>
    <label class="custom">
      <span>At a time</span>
      <input type="datetime-local" bind:value={custom} {min} />
      <button class="btn small" disabled={!custom} onclick={pickCustom}>Set</button>
    </label>
  </div>
{/if}

<style>
  .wait-picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 10px;
    padding: 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .label,
  .note span,
  .custom span {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }
  .note {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .custom {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .custom input {
    width: auto;
    flex: 1 1 180px;
  }
  .chip {
    background: var(--surface);
  }
</style>
