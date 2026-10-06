// Drag-to-reorder for vertical lists, as a Svelte action on the list container.
// - Drag starts from an element with [data-handle] inside an item with [data-id].
//   Handles use `touch-action: none`, so dragging on touch works without fighting
//   page scroll, and scrolling the list elsewhere never starts a drag.
// - Keyboard: focus a handle and press ArrowUp / ArrowDown.
// The caller persists the move in `onMove(id, newIndex)`; the list re-renders from
// the store, so no DOM is moved here (only transforms during the drag).

export type SortableOptions = { onMove: (id: string, index: number) => void; disabled?: boolean }

export function sortable(node: HTMLElement, opts: SortableOptions) {
  let options = opts
  let drag: {
    id: string
    el: HTMLElement
    items: HTMLElement[]
    from: number
    to: number
    startY: number
    height: number
    pointerId: number
  } | null = null

  const items = () => Array.from(node.querySelectorAll<HTMLElement>(':scope > [data-id]'))

  function onPointerDown(e: PointerEvent) {
    if (options.disabled || e.button !== 0) return
    const handle = (e.target as HTMLElement).closest<HTMLElement>('[data-handle]')
    if (!handle || !node.contains(handle)) return
    const el = handle.closest<HTMLElement>('[data-id]')
    if (!el || el.parentElement !== node) return
    e.preventDefault()
    const list = items()
    const from = list.indexOf(el)
    const rect = el.getBoundingClientRect()
    const gap = list.length > 1 ? Math.abs(list[1].getBoundingClientRect().top - list[0].getBoundingClientRect().bottom) : 0
    drag = {
      id: el.dataset.id!,
      el,
      items: list,
      from,
      to: from,
      startY: e.clientY,
      height: rect.height + gap,
      pointerId: e.pointerId,
    }
    handle.setPointerCapture(e.pointerId)
    el.classList.add('dragging')
    node.classList.add('sorting')
    for (const it of list) if (it !== el) it.style.transition = 'transform 150ms ease'
    handle.addEventListener('pointermove', onPointerMove)
    handle.addEventListener('pointerup', onPointerUp)
    handle.addEventListener('pointercancel', onPointerUp)
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.pointerId) return
    const dy = e.clientY - drag.startY
    drag.el.style.transform = `translateY(${dy}px)`
    const to = Math.max(0, Math.min(drag.items.length - 1, drag.from + Math.round(dy / drag.height)))
    if (to !== drag.to) {
      drag.to = to
      drag.items.forEach((it, i) => {
        if (it === drag!.el) return
        let shift = 0
        if (drag!.from < to && i > drag!.from && i <= to) shift = -drag!.height
        if (drag!.from > to && i < drag!.from && i >= to) shift = drag!.height
        it.style.transform = shift ? `translateY(${shift}px)` : ''
      })
      navigator.vibrate?.(5)
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (!drag) return
    const handle = e.currentTarget as HTMLElement
    handle.removeEventListener('pointermove', onPointerMove)
    handle.removeEventListener('pointerup', onPointerUp)
    handle.removeEventListener('pointercancel', onPointerUp)
    const { id, from, to, el } = drag
    for (const it of drag.items) {
      it.style.transition = ''
      it.style.transform = ''
    }
    el.classList.remove('dragging')
    node.classList.remove('sorting')
    drag = null
    if (to !== from && e.type === 'pointerup') options.onMove(id, to)
  }

  function onKeyDown(e: KeyboardEvent) {
    if (options.disabled) return
    const handle = (e.target as HTMLElement).closest<HTMLElement>('[data-handle]')
    if (!handle || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return
    const el = handle.closest<HTMLElement>('[data-id]')
    if (!el) return
    const list = items()
    const from = list.indexOf(el)
    const to = e.key === 'ArrowUp' ? from - 1 : from + 1
    if (to < 0 || to >= list.length) return
    e.preventDefault()
    options.onMove(el.dataset.id!, to)
    // Keep focus on the moved item's handle after re-render.
    requestAnimationFrame(() =>
      node.querySelector<HTMLElement>(`[data-id="${el.dataset.id}"] [data-handle]`)?.focus(),
    )
  }

  node.addEventListener('pointerdown', onPointerDown)
  node.addEventListener('keydown', onKeyDown)
  return {
    update(o: SortableOptions) {
      options = o
    },
    destroy() {
      node.removeEventListener('pointerdown', onPointerDown)
      node.removeEventListener('keydown', onKeyDown)
    },
  }
}
