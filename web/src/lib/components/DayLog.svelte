<script lang="ts">
  // A day's activity log (owner request): what got done, focus time, missed and
  // carried items, planning — built by the server from what was recorded.
  import { untrack } from 'svelte'
  import { api } from '../api/client'
  import type { LogItem } from '../api/types/LogItem'
  import { longDate } from '../dates'
  import { store } from '../store.svelte'
  import { toast } from '../toast.svelte'
  import Icon from './Icon.svelte'

  let { date, open = true }: { date: string; open?: boolean } = $props()
  let items = $state<LogItem[]>([])
  let loaded = $state(false)
  // Initial state only; the user toggles it from there.
  let expanded = $state(untrack(() => open))

  const ICON: Record<string, string> = {
    completed: 'check',
    skipped: 'skip',
    wont_do: 'x',
    missed: 'x',
    started: 'play',
    created: 'plus',
    focus: 'target',
    planned: 'calendar',
  }

  // Refresh (debounced) whenever something that shows up in the log changes.
  $effect(() => {
    void date
    for (const t of store.tasks.values()) void t.status, void t.started_at
    void store.focusSessions.size
    void store.dayPlans.size
    const timer = setTimeout(async () => {
      try {
        items = await api.get<LogItem[]>(`/days/${date}/log`)
      } catch {
        /* offline */
      }
      loaded = true
    }, 400)
    return () => clearTimeout(timer)
  })

  const tz = $derived(store.me?.timezone ?? 'UTC')
  const time = (iso: string, kind: string) =>
    kind === 'missed'
      ? ''
      : new Intl.DateTimeFormat('en-GB', { timeZone: tz, hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(new Date(iso))
  const focusMin = $derived(items.filter((i) => i.kind === 'focus').reduce((s, i) => s + (i.minutes ?? 0), 0))
  const done = $derived(items.filter((i) => i.kind === 'completed').length)

  function copy() {
    const text = [`Activity log — ${longDate(date)}`, ...items.map((i) => `${time(i.at, i.kind) || '     '}  ${i.text}`)].join('\n')
    navigator.clipboard?.writeText(text).then(() => toast('Copied the activity log'))
  }
</script>

<section class="log">
  <button class="section-title toggle" onclick={() => (expanded = !expanded)} aria-expanded={expanded}>
    <Icon name="list" size={14} /> Activity log
    {#if loaded}<span class="sum">{done} done{focusMin ? ` · ${focusMin} min focus` : ''}</span>{/if}
  </button>
  {#if expanded}
    {#if !items.length}
      <p class="empty card">{loaded ? 'Nothing recorded yet.' : 'Loading…'}</p>
    {:else}
      <ol class="card">
        {#each items as i, n (n)}
          <li class={i.kind}>
            <span class="time">{time(i.at, i.kind)}</span>
            <span class="ic"><Icon name={ICON[i.kind] ?? 'list'} size={14} /></span>
            <span class="text">{i.text}</span>
          </li>
        {/each}
      </ol>
      <button class="btn small copy" onclick={copy}><Icon name="list" size={14} /> Copy as text</button>
    {/if}
  {/if}
</section>

<style>
  .toggle {
    width: 100%;
  }
  .sum {
    margin-left: auto;
    text-transform: none;
    letter-spacing: 0;
    font-weight: 500;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 6px 0;
  }
  li {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 5px 14px;
    font-size: 14px;
  }
  .time {
    flex: none;
    width: 42px;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
    font-size: 12px;
  }
  .ic {
    flex: none;
    display: grid;
    color: var(--muted);
    transform: translateY(2px);
  }
  .completed .ic {
    color: var(--ok);
  }
  .missed .ic,
  .wont_do .ic {
    color: var(--danger);
  }
  .focus .ic {
    color: var(--accent);
  }
  .text {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .copy {
    margin-top: 8px;
  }
</style>
