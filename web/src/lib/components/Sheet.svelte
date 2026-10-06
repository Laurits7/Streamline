<script lang="ts">
  // Bottom sheet on phones, side panel on wide screens.
  import type { Snippet } from 'svelte'
  import Icon from './Icon.svelte'

  let { title, onclose, children, header }: { title: string; onclose: () => void; children: Snippet; header?: Snippet } =
    $props()

  let panel: HTMLDivElement
  $effect(() => {
    const prev = document.activeElement as HTMLElement | null
    panel?.focus()
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onclose()
    window.addEventListener('keydown', onKey)
    document.body.style.overflow = 'hidden'
    return () => {
      window.removeEventListener('keydown', onKey)
      document.body.style.overflow = ''
      prev?.focus?.()
    }
  })
</script>

<div class="backdrop" onclick={onclose} aria-hidden="true"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" bind:this={panel}>
  <div class="grabber" aria-hidden="true"></div>
  <header>
    {#if header}{@render header()}{:else}<h2>{title}</h2>{/if}
    <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
  </header>
  <div class="body">
    {@render children()}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    z-index: 50;
    animation: fade 160ms ease-out;
  }
  .sheet {
    position: fixed;
    z-index: 51;
    background: var(--surface);
    display: flex;
    flex-direction: column;
    outline: none;
    left: 0;
    right: 0;
    bottom: 0;
    max-height: 92dvh;
    border-radius: 18px 18px 0 0;
    box-shadow: var(--shadow-lg);
    animation: slide-up 220ms cubic-bezier(0.2, 0.9, 0.3, 1);
    padding-bottom: env(safe-area-inset-bottom);
  }
  @media (min-width: 900px) {
    .sheet {
      left: auto;
      top: 0;
      width: 440px;
      max-height: none;
      border-radius: 0;
      animation: slide-left 220ms cubic-bezier(0.2, 0.9, 0.3, 1);
    }
    .grabber {
      display: none;
    }
  }
  .grabber {
    width: 40px;
    height: 4px;
    border-radius: 2px;
    background: var(--surface-3);
    margin: 8px auto 0;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 12px 12px 8px 20px;
  }
  h2 {
    font-size: 17px;
  }
  .body {
    overflow-y: auto;
    padding: 4px 20px 24px;
    overscroll-behavior: contain;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes slide-up {
    from {
      transform: translateY(40%);
      opacity: 0;
    }
  }
  @keyframes slide-left {
    from {
      transform: translateX(40%);
      opacity: 0;
    }
  }
</style>
