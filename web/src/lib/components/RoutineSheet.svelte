<script lang="ts">
  // Create or edit a routine (SPEC §6.3). Edits apply "from a date on"; earlier
  // occurrences keep their history. Editing one occurrence happens in the task sheet.
  import { untrack } from 'svelte'
  import { fmtMinutes } from '../dates'
  import { DAY_NAMES, DAYS, describeSeries, fromRule, toRule, type Preset } from '../rrule'
  import { store } from '../store.svelte'
  import { ui } from '../ui.svelte'
  import Icon from './Icon.svelte'
  import Sheet from './Sheet.svelte'

  let { id }: { id: string } = $props()
  const existing = untrack(() => (id === 'new' ? null : (store.series.get(id) ?? null)))

  type Mode = 'repeat' | 'anchored' | 'flexible'
  let title = $state(existing?.title ?? '')
  let notes = $state(existing?.notes ?? '')
  let mode = $state<Mode>((existing?.mode as Mode) ?? 'repeat')
  let preset = $state<Preset>(fromRule(existing?.rrule ?? 'FREQ=DAILY'))
  let startTime = $state(existing?.start_time ?? '08:00')
  let duration = $state<number | null>(existing?.duration_min ?? null)
  let times = $state(existing?.times_per_window ?? 2)
  let window = $state<'week' | 'month'>((existing?.window as 'week' | 'month') ?? 'week')
  let projectId = $state(existing?.project_id ?? '')
  let typeId = $state(existing?.task_type_id ?? '')
  let estimate = $state<number | null>(existing?.estimate_min ?? null)
  let difficulty = $state<number | null>(existing?.difficulty ?? null)
  let dtstart = $state(existing?.dtstart ?? store.today)
  let until = $state(existing?.until ?? '')
  let from = $state(store.today)
  let saving = $state(false)

  const defaultType: Record<Mode, string> = { repeat: 'tt_carry_on', anchored: 'tt_expires', flexible: 'tt_window' }
  const effectiveType = $derived(typeId || defaultType[mode])
  const types = $derived([...store.taskTypes.values()])
  const projects = $derived(store.projectTree())
  const rule = $derived(toRule(preset))
  const summary = $derived(
    describeSeries({ mode, rrule: rule, start_time: startTime, times_per_window: times, window }),
  )

  const presetKinds: { kind: Preset['kind']; label: string }[] = [
    { kind: 'daily', label: 'Every day' },
    { kind: 'weekdays', label: 'Weekdays' },
    { kind: 'weekly', label: 'Certain days' },
    { kind: 'interval', label: 'Every N days' },
    { kind: 'monthly', label: 'Monthly' },
    { kind: 'yearly', label: 'Yearly' },
    { kind: 'custom', label: 'Custom rule' },
  ]
  function choose(kind: Preset['kind']) {
    const today = new Date(store.today + 'T00:00:00Z')
    preset =
      kind === 'weekly'
        ? { kind, days: preset.kind === 'weekly' ? preset.days : ['MO'] }
        : kind === 'interval'
          ? { kind, n: 2 }
          : kind === 'monthly'
            ? { kind, day: today.getUTCDate() }
            : kind === 'yearly'
              ? { kind, month: today.getUTCMonth() + 1, day: today.getUTCDate() }
              : kind === 'custom'
                ? { kind, rule }
                : { kind }
  }
  function toggleDay(d: string) {
    if (preset.kind !== 'weekly') return
    const days = preset.days.includes(d) ? preset.days.filter((x) => x !== d) : [...preset.days, d]
    preset = { kind: 'weekly', days: days.length ? days : [d] }
  }

  const close = () => (ui.routine = null)

  async function save() {
    if (!title.trim()) return
    saving = true
    const body = {
      title: title.trim(),
      notes,
      mode,
      rrule: mode === 'flexible' ? null : rule,
      start_time: mode === 'anchored' ? startTime : null,
      duration_min: mode === 'anchored' ? duration : null,
      times_per_window: mode === 'flexible' ? times : null,
      window: mode === 'flexible' ? window : null,
      project_id: projectId || null,
      task_type_id: effectiveType,
      estimate_min: estimate,
      difficulty,
      until: until || null,
    }
    const ok = existing
      ? await store.updateSeries(existing.id, { ...body, from })
      : await store.createSeries({ ...body, dtstart })
    saving = false
    if (ok) close()
  }
  function end() {
    if (existing && confirm(`End “${existing.title}”? Upcoming occurrences are removed; what's done stays in the history.`)) {
      store.endSeries(existing.id)
      close()
    }
  }
</script>

<Sheet title={existing ? 'Edit routine' : 'New routine'} onclose={close}>
  <input class="title-input" type="text" bind:value={title} placeholder="e.g. Water the plants" aria-label="Title" />

  <section>
    <h3>How often</h3>
    <div class="modes">
      <button class="mode" class:on={mode === 'repeat'} onclick={() => (mode = 'repeat')}>
        <strong>On a schedule</strong><span>Due on certain days (pay rent, bins out)</span>
      </button>
      <button class="mode" class:on={mode === 'anchored'} onclick={() => (mode = 'anchored')}>
        <strong>At a fixed time</strong><span>Every morning at 7:00 (vitamins)</span>
      </button>
      <button class="mode" class:on={mode === 'flexible'} onclick={() => (mode = 'flexible')}>
        <strong>N times a week/month</strong><span>Any day (laundry twice a week)</span>
      </button>
    </div>
  </section>

  {#if mode === 'flexible'}
    <section class="row">
      <input class="num" type="number" min="1" max="31" bind:value={times} aria-label="How many times" />
      <span>times per</span>
      <div class="chips">
        <button class="chip" class:on={window === 'week'} onclick={() => (window = 'week')}>week</button>
        <button class="chip" class:on={window === 'month'} onclick={() => (window = 'month')}>month</button>
      </div>
    </section>
  {:else}
    <section>
      <div class="chips">
        {#each presetKinds as p (p.kind)}
          <button class="chip" class:on={preset.kind === p.kind} onclick={() => choose(p.kind)}>{p.label}</button>
        {/each}
      </div>
      {#if preset.kind === 'weekly'}
        <div class="chips days">
          {#each DAYS as d (d)}
            <button class="chip" class:on={preset.days.includes(d)} aria-pressed={preset.days.includes(d)} onclick={() => toggleDay(d)}>{DAY_NAMES[d]}</button>
          {/each}
        </div>
      {:else if preset.kind === 'interval'}
        <div class="row">Every <input class="num" type="number" min="2" max="365" value={preset.n} onchange={(e) => (preset = { kind: 'interval', n: Number((e.currentTarget as HTMLInputElement).value) })} /> days</div>
      {:else if preset.kind === 'monthly'}
        <div class="row">
          On day
          <select value={preset.day} onchange={(e) => (preset = { kind: 'monthly', day: Number((e.currentTarget as HTMLSelectElement).value) })}>
            {#each Array.from({ length: 31 }, (_, i) => i + 1) as n (n)}<option value={n}>{n}</option>{/each}
            <option value={-1}>last day</option>
          </select>
        </div>
      {:else if preset.kind === 'yearly'}
        <div class="row">
          On
          <input
            type="date"
            value={`2026-${String(preset.month).padStart(2, '0')}-${String(preset.day).padStart(2, '0')}`}
            onchange={(e) => {
              const v = (e.currentTarget as HTMLInputElement).value
              if (v) preset = { kind: 'yearly', month: Number(v.slice(5, 7)), day: Number(v.slice(8, 10)) }
            }} />
        </div>
      {:else if preset.kind === 'custom'}
        <input type="text" value={preset.rule} placeholder="FREQ=MONTHLY;BYDAY=-1FR" onchange={(e) => (preset = { kind: 'custom', rule: (e.currentTarget as HTMLInputElement).value })} aria-label="RRULE" />
        <p class="muted help">An iCalendar RRULE, e.g. <code>FREQ=MONTHLY;BYDAY=-1FR</code> (last Friday of the month).</p>
      {/if}
    </section>
    {#if mode === 'anchored'}
      <section class="grid2">
        <label><span>Time</span><input type="time" bind:value={startTime} /></label>
        <label>
          <span>Duration</span>
          <select bind:value={duration}>
            <option value={null}>Use estimate</option>
            {#each [5, 10, 15, 30, 45, 60, 90, 120] as m (m)}<option value={m}>{fmtMinutes(m)}</option>{/each}
          </select>
        </label>
      </section>
    {/if}
  {/if}
  <p class="summary"><Icon name="repeat" size={14} /> {summary}</p>

  <section class="grid2">
    {#if existing}
      <label><span>Changes apply from</span><input type="date" bind:value={from} min={store.today} /></label>
    {:else}
      <label><span>Starts</span><input type="date" bind:value={dtstart} /></label>
    {/if}
    <label><span>Ends (optional)</span><input type="date" bind:value={until} /></label>
  </section>
  {#if existing}<p class="muted help">Past occurrences keep their history. To change just one day, open that day's task instead.</p>{/if}

  <section>
    <h3>Project</h3>
    <select bind:value={projectId}>
      <option value="">Inbox</option>
      {#each projects as { project: p, depth } (p.id)}<option value={p.id}>{'   '.repeat(depth)}{p.name}</option>{/each}
    </select>
  </section>

  <section>
    <h3>If not done in time</h3>
    <div class="chips">
      {#each types as t (t.id)}
        <button class="chip" class:on={effectiveType === t.id} onclick={() => (typeId = t.id)}>{t.name}</button>
      {/each}
    </div>
  </section>

  <section class="grid2">
    <div>
      <h3>Estimate</h3>
      <div class="chips">
        {#each [5, 15, 30, 60] as m (m)}
          <button class="chip" class:on={estimate === m} onclick={() => (estimate = estimate === m ? null : m)}>{fmtMinutes(m)}</button>
        {/each}
      </div>
    </div>
    <div>
      <h3>Difficulty</h3>
      <div class="chips">
        {#each ['Easy', 'Medium', 'Hard'] as label, i (label)}
          <button class="chip" class:on={difficulty === i + 1} onclick={() => (difficulty = difficulty === i + 1 ? null : i + 1)}>{label}</button>
        {/each}
      </div>
    </div>
  </section>

  <section>
    <h3>Notes</h3>
    <textarea bind:value={notes} rows="3" placeholder="Copied to each occurrence"></textarea>
  </section>

  <footer>
    {#if existing}<button class="btn danger" onclick={end}><Icon name="trash" size={16} /> End routine</button>{:else}<span></span>{/if}
    <button class="btn primary" disabled={!title.trim() || saving} onclick={save}>{existing ? 'Save' : 'Create routine'}</button>
  </footer>
</Sheet>

<style>
  .title-input {
    font-size: 19px !important;
    font-weight: 600;
  }
  section {
    margin-top: 18px;
  }
  h3 {
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
    margin-bottom: 8px;
  }
  .modes {
    display: grid;
    gap: 6px;
  }
  .mode {
    display: flex;
    flex-direction: column;
    text-align: left;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
  }
  .mode span {
    font-size: 12px;
    color: var(--muted);
  }
  .mode.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 10px;
  }
  .row select,
  .row input[type='date'] {
    width: auto;
  }
  .num {
    width: 72px !important;
  }
  .days {
    margin-top: 10px;
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--accent);
    font-weight: 600;
    margin: 14px 0 0;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  label span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .help {
    font-size: 12px;
    margin: 6px 0 0;
  }
  code {
    font-size: 12px;
  }
  footer {
    display: flex;
    justify-content: space-between;
    margin-top: 24px;
  }
</style>
