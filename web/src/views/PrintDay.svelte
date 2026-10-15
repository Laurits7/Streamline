<script lang="ts">
  // A printable day sheet (7.3): the day's fixed times and tasks with tick boxes and short
  // codes, and a QR code to tick them off on a phone afterwards (D-14).
  import { untrack } from 'svelte'
  import { api, ApiError } from '../lib/api/client'
  import type { PrintSheet } from '../lib/api/types/PrintSheet'
  import Icon from '../lib/components/Icon.svelte'
  import { timeRange } from '../lib/calendar'
  import { fmtMinutes, longDate } from '../lib/dates'
  import { store } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'

  let { date }: { date: string } = $props()
  let sheet = $state<PrintSheet | null>(null)
  let error = $state('')

  $effect(() => {
    void date
    untrack(() => {
      sheet = null
      api
        .post<PrintSheet>('/print/sheets', { date })
        .then((s) => (sheet = s))
        .catch((e) => (error = e instanceof ApiError ? e.message : 'Could not make the sheet'))
    })
  })

  const rows = $derived(
    (sheet?.items ?? [])
      .map((i) => ({ ...i, task: store.tasks.get(i.task_id), entry: store.entryForTask(i.task_id) }))
      .filter((r) => r.task),
  )
  const events = $derived(store.dayEvents(date))
</script>

<div class="screen-only bar">
  <a class="icon-btn" href={date === store.today ? '/' : `/day/${date}`} aria-label="Back"><Icon name="left" /></a>
  <h1>Print the day</h1>
  <button class="btn primary" disabled={!sheet} onclick={() => window.print()}><Icon name="check" size={16} /> Print</button>
</div>
<p class="screen-only muted note">
  Tick things off on paper, then scan the QR code with your phone to mark them done here (the link works for 48 hours,
  only for these tasks).
</p>

{#if error}
  <p class="muted">{error}</p>
{:else if !sheet}
  <p class="muted">Preparing…</p>
{:else}
  <article class="sheet">
    <header>
      <div>
        <h2>{longDate(date)}</h2>
        <p>{store.me?.display_name}</p>
      </div>
      <!-- Our own server's QR image. -->
      <div class="qr" aria-label="QR code to tick items off on a phone">{@html sheet.qr_svg}</div>
    </header>

    {#if events.allDay.length || events.timed.length}
      <section>
        <h3>Calendar</h3>
        <ul class="plain">
          {#each events.allDay as e (e.event.id)}<li>{e.event.title}</li>{/each}
          {#each events.timed as e (e.event.id)}<li><span class="time">{timeRange(e)}</span> {e.event.title}</li>{/each}
        </ul>
      </section>
    {/if}

    <section>
      <h3>To do</h3>
      {#each rows as r (r.code)}
        <div class="item" class:done={r.task?.status === 'done'}>
          <span class="box">{r.task?.status === 'done' ? '✓' : ''}</span>
          <span class="what">
            {#if r.entry?.start_time}<span class="time">{r.entry.start_time}</span>{/if}
            {r.task?.title}
            {#if r.task?.estimate_min}<span class="est">{fmtMinutes(r.task.estimate_min)}</span>{/if}
            {#if r.task?.checklist.length}
              <span class="steps">{#each r.task.checklist as s (s.id)}<span><span class="small-box">{s.done ? '✓' : ''}</span>{s.text}</span>{/each}</span>
            {/if}
          </span>
          <span class="code">{r.code}</span>
        </div>
      {:else}
        <p>Nothing planned.</p>
      {/each}
    </section>

    <section class="notes-area">
      <h3>Notes</h3>
      {#each Array.from({ length: 5 }) as _, i (i)}<div class="rule"></div>{/each}
    </section>
    <footer>Scan to tick off · valid until {new Date(sheet.expires_at).toLocaleString(store.me?.locale || undefined, { dateStyle: 'medium', timeStyle: 'short' })}</footer>
  </article>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 4px;
  }
  .bar h1 {
    font-size: 24px;
    font-weight: 750;
    flex: 1;
  }
  .note {
    font-size: 13px;
    margin: 0 0 12px;
  }
  .sheet {
    background: #fff;
    color: #111;
    max-width: 760px;
    padding: 28px 32px;
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 14px;
  }
  .sheet header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    border-bottom: 2px solid #111;
    padding-bottom: 10px;
    margin-bottom: 12px;
  }
  .sheet h2 {
    font-size: 22px;
    margin: 0;
  }
  .sheet header p {
    margin: 2px 0 0;
    color: #555;
  }
  .qr :global(svg) {
    width: 110px;
    height: 110px;
    display: block;
  }
  h3 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #555;
    margin: 14px 0 6px;
  }
  .plain {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .plain li {
    padding: 2px 0;
  }
  .item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 7px 0;
    border-bottom: 1px solid #ddd;
    break-inside: avoid;
  }
  .box {
    flex: none;
    width: 18px;
    height: 18px;
    border: 2px solid #111;
    border-radius: 3px;
    display: grid;
    place-items: center;
    font-size: 13px;
    line-height: 1;
  }
  .item.done .what {
    color: #777;
    text-decoration: line-through;
  }
  .what {
    flex: 1;
  }
  .time {
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    margin-right: 6px;
  }
  .est {
    color: #666;
    font-size: 12px;
    margin-left: 6px;
  }
  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    margin-top: 4px;
    font-size: 13px;
    color: #333;
  }
  .steps > span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .small-box {
    width: 12px;
    height: 12px;
    border: 1.5px solid #333;
    border-radius: 2px;
    display: inline-grid;
    place-items: center;
    font-size: 9px;
  }
  .code {
    flex: none;
    font-family: ui-monospace, monospace;
    font-size: 12px;
    color: #555;
  }
  .rule {
    border-bottom: 1px solid #bbb;
    height: 26px;
  }
  footer {
    margin-top: 14px;
    font-size: 11px;
    color: #777;
  }
  @media print {
    .sheet {
      border: 0;
      padding: 0;
      max-width: none;
    }
  }
</style>
