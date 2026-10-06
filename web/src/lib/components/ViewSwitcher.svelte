<script lang="ts">
  import { store } from '../store.svelte'
  import { DEFAULT_VIEW, type BoardGroup, type ViewKind, type ViewPrefs } from '../views'
  import Icon from './Icon.svelte'

  let { prefKey, groups }: { prefKey: string; groups: { id: BoardGroup; label: string }[] } = $props()
  const prefs = $derived({ ...DEFAULT_VIEW, ...store.pref<Partial<ViewPrefs>>(prefKey, {}) })
  const set = (patch: Partial<ViewPrefs>) => store.setPref(prefKey, { ...prefs, ...patch })
  const views: { id: ViewKind; label: string; icon: string }[] = [
    { id: 'list', label: 'List', icon: 'list' },
    { id: 'board', label: 'Board', icon: 'columns' },
    { id: 'matrix', label: 'Matrix', icon: 'grid' },
  ]
</script>

<div class="switcher">
  <div class="seg" role="radiogroup" aria-label="View">
    {#each views as v (v.id)}
      <button role="radio" aria-checked={prefs.view === v.id} class:on={prefs.view === v.id} onclick={() => set({ view: v.id })}>
        <Icon name={v.icon} size={15} /> <span>{v.label}</span>
      </button>
    {/each}
  </div>
  {#if prefs.view === 'board'}
    <label class="group">
      <span>Columns</span>
      <select value={prefs.group} onchange={(e) => set({ group: (e.currentTarget as HTMLSelectElement).value as BoardGroup })}>
        {#each groups as g (g.id)}<option value={g.id}>{g.label}</option>{/each}
      </select>
    </label>
  {/if}
</div>

<style>
  .switcher {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 0 0 14px;
  }
  .seg {
    display: inline-flex;
    background: var(--surface-2);
    border-radius: 10px;
    padding: 3px;
  }
  .seg button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  .group {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--muted);
  }
  .group select {
    width: auto;
    padding: 6px 10px;
  }
</style>
