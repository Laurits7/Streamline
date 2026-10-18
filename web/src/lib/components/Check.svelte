<script lang="ts">
  import Icon from './Icon.svelte'
  let {
    done,
    onclick,
    label,
    disabled = false,
    title,
  }: { done: boolean; onclick: () => void; label: string; disabled?: boolean; title?: string } = $props()
</script>

<button class="check" class:done {onclick} {disabled} {title} aria-label={label} aria-pressed={done}>
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
  /* A routine before its day (D-110). */
  .check:disabled {
    border-style: dashed;
    opacity: 0.5;
    cursor: not-allowed;
  }
  .check:disabled:hover {
    border-color: var(--faint);
    color: transparent;
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
