<script lang="ts">
  // Small persistent timer shown on every page except the focus view.
  import { clock, isWaiting, mmss, PHASE_LABEL } from '../focus.svelte'
  import { store } from '../store.svelte'
  import Icon from './Icon.svelte'

  const t = $derived(store.focusTimer)
  const running = $derived(t.running_since_ms !== null)
  const task = $derived(t.task_id ? store.tasks.get(t.task_id) : null)
</script>

{#if t.phase !== 'idle'}
  <div class="mini" class:brk={t.phase !== 'work'} role="timer" aria-label="{PHASE_LABEL[t.phase]}, {mmss(store.focusRemaining(clock.now))} left">
    <a href="/focus" class="main">
      <span class="dot" class:live={running}></span>
      <span class="time">{isWaiting() ? 'Ready' : mmss(store.focusRemaining(clock.now))}</span>
      <span class="what">{t.phase === 'work' ? (task?.title ?? 'Focus') : PHASE_LABEL[t.phase]}</span>
    </a>
    <button
      class="icon-btn"
      aria-label={running ? 'Pause' : 'Resume'}
      onclick={() => store.focus(running ? 'pause' : 'resume')}>
      <Icon name={running ? 'pause' : 'play'} size={16} />
    </button>
  </div>
{/if}

<style>
  .mini {
    position: fixed;
    right: 16px;
    bottom: calc(var(--nav-h) + 12px + env(safe-area-inset-bottom));
    z-index: 45;
    display: flex;
    align-items: center;
    gap: 2px;
    max-width: min(320px, calc(100vw - 32px));
    padding: 4px 4px 4px 14px;
    border-radius: 999px;
    background: var(--text);
    color: var(--bg);
    box-shadow: var(--shadow-lg);
  }
  @media (min-width: 900px) {
    .mini {
      bottom: 20px;
      right: 24px;
    }
  }
  .main {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
    flex: none;
  }
  .dot.live {
    background: var(--danger);
    animation: pulse 1.6s infinite;
  }
  .brk .dot.live {
    background: var(--ok);
  }
  .time {
    font-variant-numeric: tabular-nums;
    font-weight: 700;
  }
  .what {
    font-size: 13px;
    opacity: 0.8;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .icon-btn {
    color: inherit;
  }
  .icon-btn:hover {
    background: rgb(255 255 255 / 0.12);
    color: inherit;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
</style>
