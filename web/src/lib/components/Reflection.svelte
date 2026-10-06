<script lang="ts">
  // The day's reflection: free text plus three optional prompts, saved as you type.
  import { untrack } from 'svelte'
  import { api } from '../api/client'
  import { store } from '../store.svelte'

  let { date, compact = false }: { date: string; compact?: boolean } = $props()
  type Fields = { journal: string; went_well: string; went_badly: string; tomorrow: string }
  const empty: Fields = { journal: '', went_well: '', went_badly: '', tomorrow: '' }
  let form = $state<Fields>({ ...empty })
  let status = $state<'' | 'saving' | 'saved' | 'error'>('')
  let dirty = false
  let timer: ReturnType<typeof setTimeout> | undefined

  // Load the day's record (from the store, or the server for older days).
  $effect(() => {
    date
    untrack(() => {
      dirty = false
      const r = store.recordFor(date)
      form = r ? { journal: r.journal, went_well: r.went_well, went_badly: r.went_badly, tomorrow: r.tomorrow } : { ...empty }
      if (!r)
        loadRecord(date).then((rec) => {
          if (rec && !dirty) form = { journal: rec.journal, went_well: rec.went_well, went_badly: rec.went_badly, tomorrow: rec.tomorrow }
        })
    })
  })
  async function loadRecord(d: string) {
    try {
      return (await api.get<{ record: Fields | null }>(`/days/${d}/summary`)).record
    } catch {
      return null
    }
  }

  function changed() {
    dirty = true
    status = 'saving'
    clearTimeout(timer)
    timer = setTimeout(save, 700)
  }
  async function save() {
    try {
      await store.saveRecord(date, form)
      status = 'saved'
    } catch {
      status = 'error'
    }
  }
  $effect(() => () => {
    if (status === 'saving') {
      clearTimeout(timer)
      save()
    }
  })
  let prompts = $state(untrack(() => !compact))
</script>

<div class="reflection">
  <textarea rows={compact ? 3 : 5} bind:value={form.journal} oninput={changed} placeholder="How was the day? Anything worth remembering…" aria-label="Journal"></textarea>
  {#if prompts || form.went_well || form.went_badly || form.tomorrow}
    <div class="prompts">
      <label><span>What went well</span><textarea rows="2" bind:value={form.went_well} oninput={changed}></textarea></label>
      <label><span>What didn't</span><textarea rows="2" bind:value={form.went_badly} oninput={changed}></textarea></label>
      <label><span>For tomorrow</span><textarea rows="2" bind:value={form.tomorrow} oninput={changed}></textarea></label>
    </div>
  {:else}
    <button class="link" onclick={() => (prompts = true)}>Use prompts (went well / didn't / tomorrow)</button>
  {/if}
  <p class="status muted" aria-live="polite">
    {status === 'saving' ? 'Saving…' : status === 'saved' ? 'Saved' : status === 'error' ? 'Could not save, will retry when you type' : 'Only you can see this.'}
  </p>
</div>

<style>
  textarea {
    width: 100%;
    resize: vertical;
  }
  .prompts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 8px 12px;
    margin-top: 8px;
  }
  label span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 2px;
  }
  .link {
    color: var(--accent);
    font-size: 13px;
    margin-top: 6px;
  }
  .status {
    font-size: 12px;
    margin: 4px 0 0;
  }
</style>
