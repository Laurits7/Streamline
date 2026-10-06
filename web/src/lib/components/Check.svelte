<script lang="ts">
  import Icon from './Icon.svelte'
  let { done, onclick, label }: { done: boolean; onclick: () => void; label: string } = $props()
</script>

<button class="check" class:done {onclick} aria-label={label} aria-pressed={done}>
  <Icon name="check" size={14} />
</button>

<style>
  .check {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid var(--faint);
    display: grid;
    place-items: center;
    color: transparent;
    transition:
      background 150ms,
      border-color 150ms,
      transform 120ms;
    /* larger touch target */
    position: relative;
  }
  .check::after {
    content: '';
    position: absolute;
    inset: -10px;
  }
  .check:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .check:active {
    transform: scale(0.85);
  }
  .check.done {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    animation: pop 220ms ease-out;
  }
  @keyframes pop {
    50% {
      transform: scale(1.18);
    }
  }
</style>
