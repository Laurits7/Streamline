<script lang="ts">
  // Edit one time block of a day (the menu alternative to dragging it).
  import { store } from '../store.svelte'
  import Sheet from './Sheet.svelte'

  let { id, onclose }: { id: string; onclose: () => void } = $props()
  const b = $derived(store.timeBlocks.get(id))
  // svelte-ignore state_referenced_locally
  let form = $state({ title: b?.title ?? '', start: b?.start_time ?? '09:00', end: b?.end_time ?? '10:00', energy: b?.energy ?? '' })
  const valid = $derived(form.title.trim() && form.start < form.end)

  function save(e: Event) {
    e.preventDefault()
    if (!valid) return
    store.updateBlock(id, { title: form.title.trim(), start_time: form.start, end_time: form.end, energy: form.energy || null })
    onclose()
  }
</script>

<Sheet title="Time block" {onclose}>
  <form onsubmit={save}>
    <label><span>Theme</span><input type="text" bind:value={form.title} required maxlength="100" /></label>
    <div class="two">
      <label><span>From</span><input type="time" bind:value={form.start} required /></label>
      <label><span>To</span><input type="time" bind:value={form.end} required /></label>
    </div>
    <label>
      <span>Kind of work (used by “Suggest times”)</span>
      <select bind:value={form.energy}>
        <option value="">Any</option>
        <option value="hard">Hard tasks</option>
        <option value="medium">Medium tasks</option>
        <option value="easy">Easy tasks</option>
      </select>
    </label>
    {#if form.start >= form.end}<p class="err">The block must end after it starts.</p>{/if}
    <div class="row">
      <button class="btn primary" type="submit" disabled={!valid}>Save</button>
      <button class="btn danger" type="button" onclick={() => (store.deleteBlock(id), onclose())}>Remove block</button>
    </div>
  </form>
</Sheet>

<style>
  label {
    display: block;
    margin-bottom: 12px;
  }
  label span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .err {
    color: var(--danger);
    font-size: 13px;
  }
</style>
