<script lang="ts">
  // "Where am I?" — narrows the ready stack and task views to what can be done here.
  import { store } from '../store.svelte'
  import Icon from './Icon.svelte'

  let { compact = false }: { compact?: boolean } = $props()
  const places = $derived(store.placeList())
</script>

{#if places.length}
  <label class="switcher" class:compact class:set={!!store.currentPlace} title={store.useGps ? 'Detected by GPS' : 'Where are you? Lists show what can be done here'}>
    <Icon name="pin" size={15} />
    <select
      value={store.currentPlace}
      disabled={store.useGps}
      aria-label="Current place"
      onchange={(e) => store.setCurrentPlace((e.currentTarget as HTMLSelectElement).value)}>
      <option value="">{store.useGps ? 'Not at a saved place' : 'Anywhere'}</option>
      {#each places as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
    </select>
    {#if store.useGps}<span class="gps">GPS</span>{/if}
  </label>
{/if}

<style>
  .switcher {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px 4px 10px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
    max-width: 100%;
  }
  .switcher.set {
    background: var(--accent-soft);
    color: var(--accent);
  }
  select {
    border: 0 !important;
    background: transparent !important;
    padding: 2px 0 !important;
    width: auto !important;
    color: inherit;
    font-weight: 600;
    min-width: 0;
  }
  .gps {
    font-size: 10px;
    font-weight: 700;
    background: var(--accent);
    color: var(--accent-text);
    border-radius: 4px;
    padding: 0 4px;
  }
  .compact {
    font-size: 12px;
  }
</style>
