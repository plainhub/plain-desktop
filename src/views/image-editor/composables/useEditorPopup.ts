import { computed, onUnmounted, reactive, ref, watch } from 'vue'

export function popupPosition(position: { top: number; left: number }, size: { width: number; height: number }, viewport: { width: number; height: number }) {
  return {
    left: Math.max(8, Math.min(position.left, viewport.width - size.width - 8)),
    top: Math.max(8, Math.min(position.top, viewport.height - size.height - 8)),
  }
}

export function useEditorPopup(active: () => boolean, rect: () => { top: number; left: number }, width: number, close: () => void) {
  const panelRef = ref<HTMLElement | null>(null)
  const viewport = reactive({ width: window.innerWidth, height: window.innerHeight })
  const offset = reactive({ x: 0, y: 0 })
  const height = ref(0)
  const position = computed(() => popupPosition(
    { left: rect().left + offset.x, top: rect().top + offset.y },
    { width: Math.min(width, viewport.width - 16), height: height.value }, viewport,
  ))
  let dragStart = { x: 0, y: 0, left: 0, top: 0 }
  function resize() { viewport.width = window.innerWidth; viewport.height = window.innerHeight }
  function outside(event: PointerEvent) {
    if (panelRef.value?.contains(event.target as Node) || (event.target as HTMLElement).tagName === 'CANVAS') return
    close()
  }
  function move(event: PointerEvent) {
    offset.x = dragStart.left + event.clientX - dragStart.x
    offset.y = dragStart.top + event.clientY - dragStart.y
  }
  function end() {
    document.removeEventListener('pointermove', move)
    document.removeEventListener('pointerup', end)
  }
  function onDragStart(event: PointerEvent) {
    dragStart = { x: event.clientX, y: event.clientY, left: offset.x, top: offset.y }
    document.addEventListener('pointermove', move)
    document.addEventListener('pointerup', end)
  }
  watch(active, (visible, _, cleanup) => {
    if (!visible) return
    offset.x = 0; offset.y = 0
    resize()
    document.addEventListener('pointerdown', outside)
    window.addEventListener('resize', resize)
    cleanup(() => {
      end()
      document.removeEventListener('pointerdown', outside)
      window.removeEventListener('resize', resize)
    })
  }, { flush: 'post', immediate: true })
  watch(panelRef, (panel, _, cleanup) => {
    if (!panel) return
    const observer = new ResizeObserver(([entry]) => { height.value = entry!.borderBoxSize[0]!.blockSize })
    observer.observe(panel)
    cleanup(() => observer.disconnect())
  }, { flush: 'post' })
  onUnmounted(end)
  return { panelRef, posX: computed(() => position.value.left), posY: computed(() => position.value.top), onDragStart }
}
