<script lang="ts">
  // Prompt to plan (SPEC §6.2d, D-8): a clear banner at the planning time until the day is
  // planned, a gentle one when today simply isn't planned. Dismissible per day; the
  // Today view keeps working either way.
  import { addDays, nowHHMM } from '../dates'
  import { planPrompt } from '../planning'
  import { store } from '../store.svelte'
  import Icon from './Icon.svelte'

  let now = $state(nowHHMM(store.me?.timezone ?? 'UTC'))
  $effect(() => {
    const t = setInterval(() => (now = nowHHMM(store.me?.timezone ?? 'UTC')), 30_000)
    return () => clearInterval(t)
  })
  const prompt = $derived(store.me ? planPrompt(store.me, store.today, now, (d) => store.isPlanned(d)) : null)

  const key = (p: { kind: string; date: string }) => `sl.dismiss.${p.kind}.${p.date}`
  let dismissed = $state<string[]>([])
  const isDismissed = (k: string) => {
    if (dismissed.includes(k)) return true
    try {
      return localStorage.getItem(k) === '1'
    } catch {
      return false
    }
  }
  function dismiss(k: string) {
    dismissed = [...dismissed, k]
    try {
      localStorage.setItem(k, '1')
    } catch {
      /* storage unavailable */
    }
  }
  const draft = $derived(prompt ? store.planFor(prompt.date) : undefined)
</script>

{#if prompt && !isDismissed(key(prompt))}
  <div class="banner" class:soft={prompt.kind === 'unplanned'} role="status">
    <Icon name={prompt.kind === 'evening' ? 'calendar' : 'sun'} />
    <div class="text">
      <strong>
        {prompt.kind === 'evening'
          ? 'Time to plan tomorrow'
          : prompt.kind === 'morning'
            ? 'Plan your day'
            : "Today isn't planned yet"}
      </strong>
      <span>
        {draft ? 'Pick up where you left off.' : prompt.kind === 'unplanned' ? 'Your day view works anyway — planning just takes a few minutes.' : 'A few minutes now makes the day easier.'}
      </span>
    </div>
    <a class="btn small {prompt.kind === 'unplanned' ? '' : 'primary'}" href="/plan/{prompt.date}">
      {draft ? 'Continue' : prompt.date === addDays(store.today, 1) ? 'Plan tomorrow' : 'Plan now'}
    </a>
    <button class="icon-btn" aria-label="Dismiss" onclick={() => dismiss(key(prompt))}><Icon name="x" size={16} /></button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 8px 12px 16px;
    margin-bottom: 16px;
    border-radius: var(--radius);
    background: var(--accent-soft);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .banner.soft {
    background: var(--surface);
    color: var(--muted);
    border-color: var(--border);
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .text strong {
    color: var(--text);
  }
  .text span {
    font-size: 13px;
  }
</style>
