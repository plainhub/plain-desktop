import { afterEach, expect, it, vi } from 'vitest'
import { defineComponent } from 'vue'
import { useEditorPopup } from '@/views/image-editor/composables/useEditorPopup'
import { flush, renderView } from '../helpers/vue-node-renderer'

afterEach(() => vi.unstubAllGlobals())

it('keeps fractional panel height within the viewport while dragging', async () => {
  vi.stubGlobal('window', { innerWidth: 600, innerHeight: 600, addEventListener() {}, removeEventListener() {} })
  const listeners = new Map<string, (event: unknown) => void>()
  vi.stubGlobal('document', { addEventListener: (name: string, handler: (event: unknown) => void) => listeners.set(name, handler), removeEventListener() {} })
  let resize!: (entries: unknown[]) => void
  vi.stubGlobal('ResizeObserver', class { constructor(callback: (entries: unknown[]) => void) { resize = callback } observe() {} disconnect() {} })
  let popup!: ReturnType<typeof useEditorPopup>
  const view = renderView(defineComponent({ setup() {
    popup = useEditorPopup(() => true, () => ({ top: 8, left: 8 }), 320, vi.fn())
    return () => null
  } }))
  popup.panelRef.value = { offsetHeight: 337, getBoundingClientRect: () => ({ height: 320.7140625 }) } as HTMLElement
  await flush(); resize([{ borderBoxSize: [{ blockSize: 337.59375 }] }]); await flush()
  popup.onDragStart({ clientX: 0, clientY: 0 } as PointerEvent)
  listeners.get('pointermove')!({ clientX: 1000, clientY: 1000 })
  await flush()
  expect(popup.posY.value + 337.59375).toBeLessThanOrEqual(592)
  expect(popup.posX.value + 320).toBeLessThanOrEqual(592)
  view.unmount()
})
