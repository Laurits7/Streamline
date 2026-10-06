<script lang="ts">
  import Icon from './Icon.svelte'
  let { placeholder, onadd }: { placeholder: string; onadd: (title: string) => void } = $props()
  let value = $state('')

  function submit(e: Event) {
    e.preventDefault()
    const t = value.trim()
    if (!t) return
    onadd(t)
    value = ''
  }
</script>

<form class="quick" onsubmit={submit}>
  <Icon name="plus" />
  <input type="text" bind:value {placeholder} aria-label={placeholder} enterkeyhint="done" />
  {#if value.trim()}
    <button class="btn primary small" type="submit">Add</button>
  {/if}
</form>

<style>
  .quick {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 4px 6px 4px 14px;
    color: var(--muted);
    transition: border-color 120ms, box-shadow 120ms;
  }
  .quick:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  input {
    border: 0 !important;
    background: transparent !important;
    padding: 10px 0 !important;
    color: var(--text);
    box-shadow: none !important;
  }
</style>
