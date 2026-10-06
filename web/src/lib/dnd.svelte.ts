// One drag-and-drop layer for every view (SPEC §6.12), built on pointer events so
// mouse, touch and pen behave the same:
// - mouse: press on a row and move a few pixels;
// - touch: long-press a row (so normal scrolling never starts a drag), or drag the
//   grip handle straight away;
// - keyboard: focus a row's grip handle and use ArrowUp / ArrowDown (lists only).
// Drags show a floating copy of the row. Drop targets register with `droppable`
// (any element) or `dropList` (an ordered list that reports an insert index).
// Every drag action also has a non-drag alternative in the task sheet / menus.

export type DragItem =
  | { kind: 'task'; taskId: string; entryId?: string; durationMin: number; grabOffsetMin?: number }
  | { kind: 'project'; projectId: string }

export type DropAt = { x: number; y: number; el: HTMLElement }

export type DropTarget = {
  accepts: (item: DragItem) => boolean
  over?: (item: DragItem, at: DropAt) => void
  leave?: () => void
  drop: (item: DragItem, at: DropAt) => void
}

/** The item being dragged, for views that want to react (e.g. highlight targets). */
export const dnd = $state({ item: null as DragItem | null })

const targets = new WeakMap<HTMLElement, DropTarget>()

export function droppable(node: HTMLElement, target: DropTarget) {
  node.setAttribute('data-drop', '')
  targets.set(node, target)
  return {
    update(t: DropTarget) {
      targets.set(node, t)
    },
    destroy() {
      targets.delete(node)
    },
  }
}

/** Innermost drop target under the point that accepts the item. */
function targetAt(x: number, y: number, item: DragItem): [HTMLElement, DropTarget] | null {
  let el = document.elementFromPoint(x, y) as HTMLElement | null
  while (el) {
    const d = el.closest<HTMLElement>('[data-drop]')
    if (!d) return null
    const t = targets.get(d)
    if (t?.accepts(item)) return [d, t]
    el = d.parentElement
  }
  return null
}

let active = false

export type DraggableOptions = {
  item: (down: PointerEvent, node: HTMLElement) => DragItem | null
  disabled?: boolean
}

export function draggable(node: HTMLElement, opts: DraggableOptions) {
  let o = opts
  node.setAttribute('data-draggable', '')

  function onDown(e: PointerEvent) {
    if (o.disabled || active || e.button !== 0 || !e.isPrimary) return
    const t = e.target as HTMLElement
    if (t.closest('[data-nodrag], input, textarea, select')) return
    if (t.closest('[data-draggable]') !== node) return // nested draggables: innermost wins
    const fromHandle = !!t.closest('[data-handle]')
    const longPress = e.pointerType !== 'mouse' && !fromHandle
    const sx = e.clientX
    const sy = e.clientY
    let timer: ReturnType<typeof setTimeout> | undefined

    const cancel = () => {
      clearTimeout(timer)
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', cancel)
      window.removeEventListener('pointercancel', cancel)
      node.removeEventListener('contextmenu', noMenu)
    }
    const begin = (at: { clientX: number; clientY: number }) => {
      cancel()
      const item = o.item(e, node)
      if (item) start(node, item, e, at)
    }
    const onMove = (ev: PointerEvent) => {
      const dist = Math.hypot(ev.clientX - sx, ev.clientY - sy)
      if (longPress) {
        if (dist > 8) cancel() // the finger moved first: it's a scroll, not a drag
      } else if (dist > 5) begin(ev)
    }
    const noMenu = (ev: Event) => ev.preventDefault()

    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', cancel)
    window.addEventListener('pointercancel', cancel)
    if (longPress) {
      node.addEventListener('contextmenu', noMenu)
      timer = setTimeout(() => {
        navigator.vibrate?.(12)
        begin(e)
      }, 350)
    }
  }

  // Browsers decide at touchstart whether a gesture may scroll, so the listener that
  // stops scrolling once a long-press drag has begun must exist before the touch.
  const onTouchMove = (e: TouchEvent) => {
    if (active) e.preventDefault()
  }
  node.addEventListener('pointerdown', onDown)
  node.addEventListener('touchmove', onTouchMove, { passive: false })
  return {
    update(n: DraggableOptions) {
      o = n
    },
    destroy() {
      node.removeEventListener('pointerdown', onDown)
      node.removeEventListener('touchmove', onTouchMove)
    },
  }
}

function start(node: HTMLElement, item: DragItem, down: PointerEvent, at: { clientX: number; clientY: number }) {
  active = true
  dnd.item = item
  const rect = node.getBoundingClientRect()
  const ghost = node.cloneNode(true) as HTMLElement
  ghost.classList.add('drag-ghost')
  ghost.removeAttribute('data-id')
  ghost.removeAttribute('data-drop')
  Object.assign(ghost.style, {
    position: 'fixed',
    left: `${rect.left}px`,
    top: `${rect.top}px`,
    width: `${rect.width}px`,
    height: `${rect.height}px`,
    right: 'auto',
    bottom: 'auto',
    margin: '0',
    pointerEvents: 'none',
    zIndex: '1000',
  })
  document.body.appendChild(ghost)
  node.classList.add('drag-source')
  document.body.classList.add('dragging-active')
  getSelection()?.removeAllRanges()

  const offX = down.clientX - rect.left
  const offY = down.clientY - rect.top
  let x = at.clientX
  let y = at.clientY
  let current: [HTMLElement, DropTarget] | null = null
  let raf = 0

  const place = () => {
    ghost.style.transform = `translate(${x - offX - rect.left}px, ${y - offY - rect.top}px) rotate(1deg)`
  }
  const hit = () => {
    const t = targetAt(x, y, item)
    if (t?.[0] !== current?.[0]) {
      current?.[1].leave?.()
      current?.[0].classList.remove('drop-hover')
      current = t
      current?.[0].classList.add('drop-hover')
    }
    current?.[1].over?.(item, { x, y, el: current[0] })
  }
  const onMove = (e: PointerEvent) => {
    x = e.clientX
    y = e.clientY
    place()
    hit()
  }
  // Auto-scroll the window, or a scroll box marked [data-autoscroll], near its edges.
  const loop = () => {
    let scrolled = false
    const box = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest<HTMLElement>('[data-autoscroll]')
    if (box) {
      const r = box.getBoundingClientRect()
      const d = y < r.top + 40 ? y - (r.top + 40) : y > r.bottom - 40 ? y - (r.bottom - 40) : 0
      if (d) {
        box.scrollTop += d / 3
        scrolled = true
      }
    }
    const vh = window.innerHeight
    const wd = y < 56 ? y - 56 : y > vh - 80 ? y - (vh - 80) : 0
    if (wd) {
      window.scrollBy(0, wd / 3)
      scrolled = true
    }
    if (scrolled) hit()
    raf = requestAnimationFrame(loop)
  }
  const onKey = (e: KeyboardEvent) => e.key === 'Escape' && end(false)
  const onUp = () => end(true)
  const onCancel = () => end(false)

  function end(commit: boolean) {
    cancelAnimationFrame(raf)
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    window.removeEventListener('pointercancel', onCancel)
    window.removeEventListener('keydown', onKey)
    ghost.remove()
    node.classList.remove('drag-source')
    document.body.classList.remove('dragging-active')
    const target = current
    current?.[1].leave?.()
    current?.[0].classList.remove('drop-hover')
    current = null
    active = false
    dnd.item = null
    // The click that follows a drag must not open the task.
    const swallow = (e: Event) => {
      e.stopPropagation()
      e.preventDefault()
    }
    window.addEventListener('click', swallow, { capture: true })
    setTimeout(() => window.removeEventListener('click', swallow, { capture: true }), 60)
    if (commit && target) target[1].drop(item, { x, y, el: target[0] })
  }

  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  window.addEventListener('pointercancel', onCancel)
  window.addEventListener('keydown', onKey)
  place()
  hit()
  raf = requestAnimationFrame(loop)
}

export type DropListOptions = {
  accepts: (item: DragItem) => boolean
  /** `index` counts the list's items, not including the dragged one. */
  drop: (item: DragItem, index: number) => void
  /** Keyboard reorder from a focused [data-handle]: move item `id` to `index`. */
  keyMove?: (id: string, index: number) => void
}

/** An ordered list (children with [data-id]) that accepts drops at a position. */
export function dropList(node: HTMLElement, opts: DropListOptions) {
  let o = opts
  let marked: HTMLElement | null = null
  const rows = () =>
    Array.from(node.querySelectorAll<HTMLElement>(':scope > [data-id]')).filter((c) => !c.classList.contains('drag-source'))
  const clear = () => {
    marked?.classList.remove('drop-before', 'drop-after')
    marked = null
  }
  const indexAt = (y: number): [number, HTMLElement[]] => {
    const rs = rows()
    let i = 0
    for (const r of rs) {
      const b = r.getBoundingClientRect()
      if (y > b.top + b.height / 2) i++
    }
    return [i, rs]
  }
  const d = droppable(node, {
    accepts: (it) => o.accepts(it),
    over: (_, at) => {
      const [i, rs] = indexAt(at.y)
      clear()
      if (rs.length) {
        marked = rs[Math.min(i, rs.length - 1)]
        marked.classList.add(i < rs.length ? 'drop-before' : 'drop-after')
      }
    },
    leave: clear,
    drop: (it, at) => {
      const [i] = indexAt(at.y)
      clear()
      o.drop(it, i)
    },
  })
  const onKey = (e: KeyboardEvent) => {
    if (!o.keyMove || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return
    const handle = (e.target as HTMLElement).closest<HTMLElement>('[data-handle]')
    const row = handle?.closest<HTMLElement>('[data-id]')
    if (!row || row.parentElement !== node) return
    const rs = rows()
    const from = rs.indexOf(row)
    const to = e.key === 'ArrowUp' ? from - 1 : from + 1
    if (from < 0 || to < 0 || to >= rs.length) return
    e.preventDefault()
    o.keyMove(row.dataset.id!, to)
    requestAnimationFrame(() => node.querySelector<HTMLElement>(`[data-id="${row.dataset.id}"] [data-handle]`)?.focus())
  }
  node.addEventListener('keydown', onKey)
  return {
    update(n: DropListOptions) {
      o = n
    },
    destroy() {
      d.destroy()
      node.removeEventListener('keydown', onKey)
    },
  }
}
