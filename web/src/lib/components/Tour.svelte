<script lang="ts">
  // The guided tour (D-77): one card per step, over the page it talks about, with the thing
  // it describes highlighted. Opened on first sign-in and from Help or Settings.
  import { tick } from 'svelte'
  import { router } from '../router.svelte'
  import { store } from '../store.svelte'
  import { placeCard, TOUR, TOUR_PREF, type Rect } from '../tour'
  import { ui } from '../ui.svelte'

  const PAD = 6
  const step = $derived(ui.tour === null ? null : TOUR[ui.tour])
  const last = $derived(ui.tour === TOUR.length - 1)

  let target = $state<Element | null>(null)
  /** The step's highlight was looked for (and maybe not found). */
  let searched = $state(false)
  let rect = $state<Rect | null>(null)
  let card = $state<HTMLElement>()
  let cardW = $state(0)
  let cardH = $state(0)
  let view = $state({ width: window.innerWidth, height: window.innerHeight })

  const visible = (el: Element) => {
    const r = el.getBoundingClientRect()
    return r.width > 0 && r.height > 0
  }
  const find = (selectors: string[]) => {
    for (const s of selectors) {
      const el = [...document.querySelectorAll(s)].find(visible)
      if (el) return el
    }
    return null
  }
  function measure() {
    view = { width: window.innerWidth, height: window.innerHeight }
    if (!target?.isConnected) return (rect = null)
    const r = target.getBoundingClientRect()
    rect = { top: r.top - PAD, left: r.left - PAD, width: r.width + 2 * PAD, height: r.height + 2 * PAD }
  }

  // Show the step's page, then wait for the highlighted element (pages load lazily).
  $effect(() => {
    const s = step
    if (!s) return
    let stop = false
    target = null
    rect = null
    searched = false
    if (router.path !== s.route) router.go(s.route)
    const started = Date.now()
    const look = () => {
      if (stop) return
      const el = s.targets ? find(s.targets) : null
      if (el) {
        el.scrollIntoView({ block: 'center', behavior: 'instant' })
        target = el
        measure()
      } else if (s.targets && Date.now() - started < 500) requestAnimationFrame(look)
      else searched = true
    }
    requestAnimationFrame(look)
    tick().then(() => card?.focus())
    return () => (stop = true)
  })

  $effect(() => {
    if (!step) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close()
      else if (e.key === 'ArrowRight') next()
      else if (e.key === 'ArrowLeft') back()
    }
    window.addEventListener('keydown', onKey)
    window.addEventListener('resize', measure)
    window.addEventListener('scroll', measure, true)
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('resize', measure)
      window.removeEventListener('scroll', measure, true)
    }
  })

  const pos = $derived(placeCard(rect, { width: cardW, height: cardH }, view))

  function close() {
    ui.tour = null
    if (!store.pref(TOUR_PREF, null)) store.setPref(TOUR_PREF, { seen: store.today })
  }
  function next() {
    if (ui.tour === null) return
    if (last) {
      close()
      router.go('/')
    } else ui.tour++
  }
  function back() {
    if (ui.tour) ui.tour--
  }
</script>

{#if step && ui.tour !== null}
  <div class="blocker" aria-hidden="true"></div>
  {#if rect}
    <div class="spot" style:top="{rect.top}px" style:left="{rect.left}px" style:width="{rect.width}px" style:height="{rect.height}px" aria-hidden="true"></div>
  {:else}
    <div class="dim" aria-hidden="true"></div>
  {/if}
  <div
    class="tour card"
    role="dialog"
    aria-modal="true"
    aria-labelledby="tour-title"
    aria-describedby="tour-body"
    tabindex="-1"
    bind:this={card}
    bind:clientWidth={cardW}
    bind:clientHeight={cardH}
    style:top="{pos.top}px"
    style:left="{pos.left}px">
    {#if ui.tour > 0}<p class="count muted">{ui.tour} of {TOUR.length - 1}</p>{/if}
    <h2 id="tour-title">{step.title}</h2>
    <p id="tour-body">{step.body}</p>
    {#if step.where && !target && searched}<p class="where muted">{step.where}</p>{/if}
    <div class="actions">
      {#if ui.tour === 0}
        <button class="btn" onclick={close}>Not now</button>
        <button class="btn primary" onclick={next}>Show me around</button>
      {:else}
        {#if !last}<button class="skip" onclick={close}>End tour</button>{/if}
        <button class="btn" onclick={back}>Back</button>
        <button class="btn primary" onclick={next}>{last ? 'Done' : 'Next'}</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .blocker {
    position: fixed;
    inset: 0;
    z-index: 60;
  }
  .dim {
    position: fixed;
    inset: 0;
    z-index: 61;
    background: rgb(0 0 0 / 0.45);
    pointer-events: none;
  }
  .spot {
    position: fixed;
    z-index: 61;
    border-radius: var(--radius);
    box-shadow:
      0 0 0 3px var(--accent),
      0 0 0 9999px rgb(0 0 0 / 0.45);
    pointer-events: none;
    transition:
      top 180ms,
      left 180ms,
      width 180ms,
      height 180ms;
  }
  .tour {
    position: fixed;
    z-index: 62;
    width: min(360px, calc(100vw - 24px));
    padding: 18px 18px 14px;
    box-shadow: var(--shadow-lg);
    outline: none;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 17px;
  }
  p {
    margin: 0;
    line-height: 1.45;
  }
  .where {
    margin-top: 8px;
    font-size: 14px;
  }
  .count {
    font-size: 12px;
    font-weight: 600;
    margin-bottom: 4px;
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
  .skip {
    margin-right: auto;
    padding: 4px 0;
    background: none;
    border: 0;
    color: var(--muted);
    font: inherit;
    font-size: 14px;
    text-decoration: underline;
    cursor: pointer;
  }
  @media (prefers-reduced-motion: reduce) {
    .spot {
      transition: none;
    }
  }
</style>
