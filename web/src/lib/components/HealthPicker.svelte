<script lang="ts">
  // The day's health check-in (D-75): one answer per day; sick and injured ask what or where.
  import { untrack } from 'svelte'
  import { hasKinds, healthLabel, KINDS, STATUSES } from '../health'
  import { store } from '../store.svelte'

  let { date, compact = false }: { date: string; compact?: boolean } = $props()
  const h = $derived(store.healthOn(date))
  let note = $state('')
  $effect.pre(() => {
    const d = date
    note = untrack(() => store.healthOn(d)?.note ?? '')
  })
  // Picking sick/injured opens the "what"/"where" step until a kind is chosen.
  let open = $state(false)

  function pick(status: string) {
    if (h?.status === status && !hasKinds(status)) return
    store.setHealth(date, status, h?.status === status ? h.kind : null, hasKinds(status) && h?.status === status ? note : '')
    if (!hasKinds(status)) note = ''
    open = hasKinds(status)
  }
  function pickKind(kind: string) {
    if (!h) return
    store.setHealth(date, h.status, h.kind === kind ? null : kind, note)
    if (kind !== 'other') open = false
  }
  function saveNote() {
    if (h && note.trim() !== h.note) store.setHealth(date, h.status, h.kind, note)
  }
</script>

<div class="health" class:compact>
  <div class="statuses" role="radiogroup" aria-label="How are you{date === store.today ? ' today' : ''}?">
    {#each STATUSES as s (s.id)}
      <button
        class="chip"
        role="radio"
        aria-checked={h?.status === s.id}
        class:on={h?.status === s.id}
        class:bad={h?.status === s.id && (s.id === 'sick' || s.id === 'injured')}
        onclick={() => pick(s.id)}><span aria-hidden="true">{s.emoji}</span> {s.label}</button>
    {/each}
  </div>
  {#if h && hasKinds(h.status) && (open || !h.kind || h.kind === 'other')}
    <div class="kinds" role="group" aria-label={h.status === 'sick' ? 'What is it?' : 'Where?'}>
      <span class="q muted">{h.status === 'sick' ? 'What is it?' : 'Where?'}</span>
      {#each KINDS[h.status] as k (k.id)}
        <button class="chip small" class:on={h.kind === k.id} aria-pressed={h.kind === k.id} onclick={() => pickKind(k.id)}>{k.label}</button>
      {/each}
    </div>
  {/if}
  {#if h && hasKinds(h.status) && (open || h.kind === 'other' || h.note)}
    <input
      class="note"
      type="text"
      maxlength="200"
      bind:value={note}
      onblur={saveNote}
      onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()}
      placeholder={h.kind === 'other' ? (h.status === 'sick' ? 'What is it?' : 'Where, what happened?') : 'A note (optional)'}
      aria-label="Health note" />
  {/if}
  {#if h}
    <p class="now muted">
      {healthLabel(h)}
      {#if hasKinds(h.status) && h.kind && !open}<button class="link" onclick={() => (open = true)}>change</button>{/if}
      <button class="link" onclick={() => store.clearHealth(date)}>clear</button>
    </p>
  {/if}
</div>

<style>
  .health {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .statuses,
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .compact .chip {
    min-height: 30px;
    padding: 0 10px;
  }
  .chip.small {
    min-height: 28px;
    font-size: 12px;
  }
  .chip.bad {
    background: var(--warn-soft);
    border-color: var(--warn);
    color: var(--warn);
  }
  .q {
    font-size: 12px;
    font-weight: 600;
  }
  .note {
    font-size: 14px;
  }
  .now {
    margin: 0;
    font-size: 12px;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .link {
    color: var(--accent);
    font-weight: 600;
  }
</style>
