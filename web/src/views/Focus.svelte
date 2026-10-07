<script lang="ts">
  // Distraction-free focus view (SPEC §6.9): one task, a big timer, and the task's
  // details. The timer state is the server's, so it matches on every device.
  import Icon from '../lib/components/Icon.svelte'
  import { addDays, fmtMinutes } from '../lib/dates'
  import { clock, isWaiting, mmss, PHASE_LABEL } from '../lib/focus.svelte'
  import { router } from '../lib/router.svelte'
  import { store } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import { ui } from '../lib/ui.svelte'
  import WaitPicker from '../lib/components/WaitPicker.svelte'
  import Checklist from '../lib/components/Checklist.svelte'

  const t = $derived(store.focusTimer)
  const task = $derived(t.task_id ? store.tasks.get(t.task_id) : null)
  const running = $derived(t.running_since_ms !== null)
  const remaining = $derived(store.focusRemaining(clock.now))
  const total = $derived(Math.max(1, t.length_min * 60_000))
  const progress = $derived(t.phase === 'idle' ? 0 : 1 - remaining / total)
  const longEvery = $derived(store.me?.focus_long_every ?? 4)

  // Today's open planned tasks, in plan order: the "next" candidates and the picker.
  const todays = $derived(
    store
      .dayEntries(store.today)
      .map((e) => store.tasks.get(e.task_id))
      .filter((x): x is NonNullable<typeof x> => !!x && x.status === 'open' && !x.waiting_since),
  )
  const inProgress = $derived([...store.tasks.values()].filter((x) => x.status === 'open' && x.started_at && !x.waiting_since && !todays.includes(x)))
  const next = $derived(todays.find((x) => x.id !== task?.id) ?? null)
  const todayStart = $derived(new Date(Date.now() - 864e5).toISOString())
  const sessionsToday = $derived(task ? store.sessions({ taskId: task.id, kind: 'work', since: todayStart }) : [])

  const R = 120
  const C = 2 * Math.PI * R

  function done() {
    if (!task) return
    store.updateTask(task.id, { status: 'done' })
    if (next) store.focus('start', next.id)
    else store.focus('stop')
    toast(`Completed “${task.title}”`)
  }
  // Handed off (a training run, a reply): wait for results and move on (D-70).
  let waitOpen = $state(false)
  function waited() {
    if (!task) return
    waitOpen = false
    if (next) store.focus('start', next.id)
    else store.focus('stop')
    toast(`Waiting for results on “${task.title}”`)
  }
  function snooze() {
    if (!task) return
    store.plan(task.id, addDays(store.today, 1))
    store.focus('stop')
    toast(`“${task.title}” moved to tomorrow`)
  }
  const leave = () => (history.length > 1 ? history.back() : router.go('/'))
</script>

<div class="focus" class:brk={t.phase !== 'work' && t.phase !== 'idle'}>
  <header>
    <button class="icon-btn" onclick={leave} aria-label="Leave focus view"><Icon name="left" /></button>
    <span class="phase">{isWaiting() ? 'Ready for the next round' : PHASE_LABEL[t.phase]}</span>
    <span class="spacer"></span>
  </header>

  {#if t.phase === 'idle' && !task}
    <section class="pick">
      <h1>What do you want to focus on?</h1>
      {#if todays.length || inProgress.length}
        <div class="card list">
          {#each [...todays, ...inProgress] as x (x.id)}
            <button class="option" onclick={() => store.focus('start', x.id)}>
              <Icon name="play" size={16} />
              <span class="t">{x.title}</span>
              {#if x.estimate_min}<span class="muted">{fmtMinutes(x.estimate_min)}</span>{/if}
            </button>
          {/each}
        </div>
      {:else}
        <p class="muted">Nothing planned for today. Plan your day, or just start a timer.</p>
      {/if}
      <button class="btn" onclick={() => store.focus('start', null)}><Icon name="target" size={16} /> Just focus, no task</button>
    </section>
  {:else}
    <section class="timer" aria-live="off">
      <svg viewBox="0 0 280 280" class="ring" aria-hidden="true">
        <circle cx="140" cy="140" r={R} class="track" />
        <circle cx="140" cy="140" r={R} class="bar" stroke-dasharray={C} stroke-dashoffset={C * (1 - progress)} />
      </svg>
      <div class="readout" role="timer" aria-label="{mmss(remaining)} left">
        <span class="big">{t.phase === 'idle' ? '0:00' : mmss(remaining)}</span>
        <span class="dots" aria-label="{t.cycle_done} of {longEvery} focus intervals until a long break">
          {#each Array.from({ length: longEvery }, (_, i) => i) as i (i)}<i class:on={i < t.cycle_done}></i>{/each}
        </span>
      </div>
    </section>

    <div class="controls">
      {#if t.phase === 'idle'}
        <button class="btn primary big" onclick={() => store.focus('start', task?.id ?? null)}><Icon name="play" /> Start focus</button>
      {:else if running}
        <button class="btn primary big" onclick={() => store.focus('pause')}><Icon name="pause" /> Pause</button>
      {:else}
        <button class="btn primary big" onclick={() => store.focus('resume')}><Icon name="play" /> {isWaiting() ? 'Start' : 'Resume'}</button>
      {/if}
      {#if t.phase !== 'idle'}
        <button class="btn" onclick={() => store.focus('skip')} title={t.phase === 'work' ? 'Skip to the break' : 'Skip the break'}><Icon name="skip" size={16} /> Skip</button>
        <button class="btn" onclick={() => store.focus('stop')}><Icon name="stop" size={16} /> Stop</button>
      {/if}
    </div>

    {#if task}
      <article class="task card">
        <h1>{task.title}</h1>
        {#if task.project_id}<p class="muted path">{store.projectPath(task.project_id)}</p>{/if}
        <p class="stats">
          <span><Icon name="clock" size={14} /> {fmtMinutes(task.actual_min)} spent{task.estimate_min ? ` of ${fmtMinutes(task.estimate_min)}` : ''}</span>
          <span>{sessionsToday.length} focus interval{sessionsToday.length === 1 ? '' : 's'} today</span>
        </p>
        {#if task.checklist.length}
          <Checklist items={task.checklist} onchange={(checklist) => store.updateTask(task.id, { checklist })} ontick={(item) => store.tickItem(task.id, item)} />
        {/if}
        {#if task.notes.trim()}<div class="notes">{task.notes}</div>{/if}
        <div class="row">
          <button class="btn primary" onclick={done}><Icon name="check" size={16} /> Done</button>
          <button class="btn" aria-expanded={waitOpen} onclick={() => (waitOpen = !waitOpen)}><Icon name="hourglass" size={16} /> Wait for results…</button>
          <button class="btn" onclick={snooze}>Snooze to tomorrow</button>
          <button class="btn" onclick={() => (ui.editing = task.id)}><Icon name="edit" size={16} /> Details</button>
          {#if next}
            <button class="btn" onclick={() => store.focus('start', next.id)}>Next: {next.title} <Icon name="right" size={14} /></button>
          {/if}
        </div>
        {#if waitOpen}<WaitPicker taskId={task.id} ondone={waited} />{/if}
      </article>
    {:else if t.phase !== 'idle'}
      <p class="muted center">Focusing without a task.</p>
    {/if}
  {/if}
</div>

<style>
  .focus {
    min-height: 100dvh;
    max-width: 640px;
    margin: 0 auto;
    padding: calc(12px + env(safe-area-inset-top)) 16px calc(32px + env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: 18px;
    --ring: var(--accent);
  }
  .focus.brk {
    --ring: var(--ok);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .phase {
    flex: 1;
    text-align: center;
    font-weight: 700;
    color: var(--ring);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 13px;
  }
  .spacer {
    width: 36px;
  }
  .timer {
    position: relative;
    width: min(280px, 72vw);
    aspect-ratio: 1;
    margin: 8px auto 0;
  }
  .ring {
    width: 100%;
    height: 100%;
    transform: rotate(-90deg);
  }
  .ring circle {
    fill: none;
    stroke-width: 12;
  }
  .track {
    stroke: var(--surface-3);
  }
  .bar {
    stroke: var(--ring);
    stroke-linecap: round;
    transition: stroke-dashoffset 1s linear;
  }
  .readout {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }
  .big {
    font-size: clamp(44px, 14vw, 64px);
    font-weight: 750;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
  }
  .dots {
    display: flex;
    gap: 6px;
  }
  .dots i {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--surface-3);
  }
  .dots i.on {
    background: var(--ring);
  }
  .controls {
    display: flex;
    justify-content: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .big.btn,
  .btn.big {
    min-width: 150px;
    min-height: 48px;
    font-size: 16px;
  }
  .task {
    padding: 18px;
  }
  .task h1,
  .pick h1 {
    font-size: 22px;
  }
  .path {
    margin: 4px 0 0;
    font-size: 13px;
  }
  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    font-size: 13px;
    color: var(--muted);
  }
  .stats span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .notes {
    white-space: pre-wrap;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    padding: 12px;
    font-size: 14px;
    max-height: 40vh;
    overflow: auto;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 14px;
  }
  .pick {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin-top: 24px;
  }
  .list {
    overflow: hidden;
  }
  .option {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 14px;
    text-align: left;
    border-bottom: 1px solid var(--border);
    color: var(--accent);
  }
  .option:last-child {
    border-bottom: 0;
  }
  .option:hover {
    background: var(--surface-2);
  }
  .option .t {
    flex: 1;
    color: var(--text);
  }
  .center {
    text-align: center;
  }
</style>
