import { mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'
import overlaySource from '@/views/screen-capture/ScreenCaptureOverlay.vue?raw'
import ScreenCaptureOverlay from '@/views/screen-capture/ScreenCaptureOverlay.vue'
import ScreenCaptureToolbar from '@/views/screen-capture/ScreenCaptureToolbar.vue'

vi.mock('@/views/image-editor/pixi/PixiEditorRenderer', () => ({
  PixiEditorRenderer: class {
    isReady = false
    async init() {
      this.isReady = true
    }
    resize() {}
    setViewport() {}
    sync() {}
    paint() {
      return this.isReady
    }
    destroy() {
      this.isReady = false
    }
  },
}))

const mounted: VueWrapper[] = []

afterEach(() => {
  while (mounted.length) mounted.pop()!.unmount()
  vi.restoreAllMocks()
})

function createFrame(width = 100, height = 80): ImageData {
  const data = new Uint8ClampedArray(width * height * 4)
  for (let offset = 0; offset < data.length; offset += 4) {
    data[offset] = 12
    data[offset + 1] = 34
    data[offset + 2] = 56
    data[offset + 3] = 255
  }
  return new ImageData(data, width, height)
}

function rect(width = 100, height = 80): DOMRect {
  return {
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: width,
    bottom: height,
    width,
    height,
    toJSON: () => ({}),
  }
}

async function pointer(element: Element, type: string, x: number, y: number, pointerId = 1) {
  element.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y, pointerId }))
  await nextTick()
}

async function mountOverlay(
  onExport = vi.fn(async () => {}),
  onCancel = vi.fn(async () => {}),
  frame = createFrame(),
  viewport = rect(),
  visibleArea?: { x: number; y: number; width: number; height: number }
) {
  const wrapper = mount(ScreenCaptureOverlay, {
    attachTo: document.body,
    props: { frame, onExport, onCancel, visibleArea },
  })
  mounted.push(wrapper)
  await nextTick()
  const stage = wrapper.get('[data-testid="capture-stage"]')
  Object.defineProperty(stage.element, 'getBoundingClientRect', { value: () => viewport })
  for (const canvas of wrapper.findAll('canvas')) Object.defineProperty(canvas.element, 'getBoundingClientRect', { value: () => viewport })
  await nextTick()
  return { wrapper, stage, onExport, onCancel, frame }
}

async function createSelection(stage: ReturnType<VueWrapper['get']>, start = { x: 10, y: 10 }, end = { x: 70, y: 60 }) {
  await pointer(stage.element, 'pointerdown', start.x, start.y)
  await pointer(stage.element, 'pointermove', end.x, end.y)
  await pointer(stage.element, 'pointerup', end.x, end.y)
}

describe('ScreenCaptureToolbar', () => {
  it('exposes exactly the pictured tools, five colors, three widths, and terminal actions', () => {
    const wrapper = mount(ScreenCaptureToolbar, {
      props: {
        activeTool: 'rect',
        color: '#ef4444',
        strokeWidth: 4,
        canUndo: false,
        canRedo: false,
        busy: false,
        canConfirm: true,
      },
    })
    mounted.push(wrapper)

    expect(wrapper.findAll('[data-tool]').map((button) => button.attributes('aria-label'))).toEqual(['Rectangle', 'Ellipse', 'Arrow', 'Pen', 'Text', 'Mosaic'])
    expect(wrapper.findAll('[data-color]').map((button) => button.attributes('data-color'))).toEqual(['#ef4444', '#eab308', '#22c55e', '#3b82f6', '#000000'])
    expect(wrapper.findAll('[data-stroke-width]')).toHaveLength(3)
    expect(wrapper.findAll('[data-action]').map((button) => button.attributes('data-action'))).toEqual(['save', 'copy', 'cancel', 'confirm'])
  })
})

describe('ScreenCaptureOverlay', () => {
  it('prefers the native work area when WebKit reports the Dock as usable', async () => {
    vi.spyOn(window, 'screenX', 'get').mockReturnValue(0)
    vi.spyOn(window, 'screenY', 'get').mockReturnValue(0)
    vi.spyOn(window.screen, 'availLeft', 'get').mockReturnValue(0)
    vi.spyOn(window.screen, 'availTop', 'get').mockReturnValue(0)
    vi.spyOn(window.screen, 'availWidth', 'get').mockReturnValue(1000)
    vi.spyOn(window.screen, 'availHeight', 'get').mockReturnValue(800)
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function () {
      return this.classList.contains('screen-capture-overlay__toolbar') ? rect(620, 56) : rect(1000, 800)
    })

    const { wrapper, stage } = await mountOverlay(
      vi.fn(async () => {}),
      vi.fn(async () => {}),
      createFrame(1000, 800),
      rect(1000, 800),
      { x: 0, y: 24, width: 1000, height: 676 }
    )
    await createSelection(stage, { x: 100, y: 500 }, { x: 800, y: 680 })
    await nextTick()

    expect(wrapper.get('.screen-capture-overlay__toolbar').attributes('style')).toContain('left: 140px')
    expect(wrapper.get('.screen-capture-overlay__toolbar').attributes('style')).toContain('top: 436px')
  })

  it('releases the decoded frame prop after installing pixels into the owned source canvas', async () => {
    const { wrapper } = await mountOverlay()

    expect(wrapper.emitted('frameInstalled')).toHaveLength(1)
    await wrapper.setProps({ frame: null })

    const source = wrapper.get<HTMLCanvasElement>('.screen-capture-overlay__source').element
    expect(source.width).toBe(100)
    expect(source.height).toBe(80)
  })

  it('has no capture-shell dependency on routing, stores, networking, GraphQL, or persistence', () => {
    for (const forbidden of ['vue-router', 'pinia', 'gql-client', 'fetch(', 'XMLHttpRequest', 'useImageEditorPersistence', '@/stores/', '@/plugins/eventbus']) {
      expect(overlaySource).not.toContain(forbidden)
    }
  })

  it('switches every capture annotation tool without adding a persistence-aware editor surface', async () => {
    const { wrapper, stage } = await mountOverlay()
    await createSelection(stage)

    for (const tool of ['rect', 'ellipse', 'arrow', 'brush', 'text', 'mosaic']) {
      await wrapper.get(`[data-tool="${tool}"]`).trigger('click')
      expect(wrapper.get(`[data-tool="${tool}"]`).attributes('aria-pressed')).toBe('true')
    }
  })

  it.each(['rect', 'ellipse', 'arrow', 'brush', 'text', 'mosaic'] as const)('commits %s as one undoable annotation operation', async (tool) => {
    const { wrapper, stage } = await mountOverlay()
    await createSelection(stage)
    await wrapper.get(`[data-tool="${tool}"]`).trigger('click')

    await pointer(stage.element, 'pointerdown', 20, 20, 31)
    if (tool === 'text') {
      await pointer(stage.element, 'pointerup', 20, 20, 31)
      const editor = wrapper.get('[aria-label="Annotation text"]')
      await editor.setValue('capture note')
      editor.element.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Enter' }))
    } else {
      if (tool === 'brush') await pointer(stage.element, 'pointermove', 35, 32, 31)
      await pointer(stage.element, 'pointermove', 50, 45, 31)
      await pointer(stage.element, 'pointerup', 50, 45, 31)
    }
    await nextTick()

    expect(wrapper.get('[aria-label="Undo"]').attributes()).not.toHaveProperty('disabled')
    await wrapper.get('[aria-label="Undo"]').trigger('click')
    expect(wrapper.get('[aria-label="Redo"]').attributes()).not.toHaveProperty('disabled')
  })

  it('captures one pointer id and rolls a selection back on pointercancel', async () => {
    const { wrapper, stage } = await mountOverlay()
    await createSelection(stage)
    expect(wrapper.get('[data-testid="selection-dimensions"]').text()).toBe('60 × 50')
    expect(wrapper.findAll('[data-handle]')).toHaveLength(8)

    await pointer(stage.element, 'pointerdown', 30, 30, 7)
    await pointer(stage.element, 'pointermove', 50, 40, 7)
    expect(wrapper.get('[data-testid="selection-dimensions"]').text()).toBe('60 × 50')
    await pointer(stage.element, 'pointercancel', 50, 40, 7)

    expect(wrapper.get('[data-testid="selection-dimensions"]').text()).toBe('60 × 50')
    expect(wrapper.get('[data-testid="selection-chrome"]').attributes('style')).toContain('left: 10px')
  })

  it('does not retain an annotation or undo entry after pointercancel', async () => {
    const { wrapper, stage } = await mountOverlay()
    await createSelection(stage)
    await wrapper.get('[data-tool="rect"]').trigger('click')

    await pointer(stage.element, 'pointerdown', 20, 20, 9)
    await pointer(stage.element, 'pointermove', 50, 45, 9)
    await pointer(stage.element, 'pointercancel', 50, 45, 9)

    expect(wrapper.get('[aria-label="Undo"]').attributes()).toHaveProperty('disabled')
  })

  it('cancels a text draft before cancelling capture and never confirms from its input', async () => {
    const { wrapper, stage, onCancel, onExport } = await mountOverlay()
    await createSelection(stage)
    await wrapper.get('[data-tool="text"]').trigger('click')
    await pointer(stage.element, 'pointerdown', 25, 25, 11)
    await pointer(stage.element, 'pointerup', 25, 25, 11)

    const editor = wrapper.get('[aria-label="Annotation text"]')
    await editor.setValue('draft')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))
    stage.element.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, clientX: 25, clientY: 25 }))
    await nextTick()
    expect(onExport).not.toHaveBeenCalled()

    editor.element.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Escape' }))
    await nextTick()
    expect(wrapper.find('[aria-label="Annotation text"]').exists()).toBe(false)
    expect(onCancel).not.toHaveBeenCalled()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await nextTick()
    expect(onCancel).toHaveBeenCalledTimes(1)
  })

  it('blocks confirm during a gesture, then supports keyboard undo/redo and confirm', async () => {
    const { wrapper, stage, onExport } = await mountOverlay()
    await createSelection(stage)
    await wrapper.get('[data-tool="rect"]').trigger('click')

    await pointer(stage.element, 'pointerdown', 20, 20, 13)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))
    stage.element.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, clientX: 30, clientY: 30 }))
    await nextTick()
    expect(onExport).not.toHaveBeenCalled()
    await pointer(stage.element, 'pointermove', 50, 45, 13)
    await pointer(stage.element, 'pointerup', 50, 45, 13)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true }))
    await nextTick()
    expect(wrapper.get('[aria-label="Redo"]').attributes()).not.toHaveProperty('disabled')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, shiftKey: true }))
    await nextTick()
    expect(wrapper.get('[aria-label="Undo"]').attributes()).not.toHaveProperty('disabled')

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))
    await vi.waitFor(() => expect(onExport).toHaveBeenCalledTimes(1))
    expect(onExport.mock.calls[0]?.[0]).toBe('confirm')
  })

  it('confirms on a selection double-click only after the active gesture completes', async () => {
    const { stage, onExport } = await mountOverlay()
    await createSelection(stage)

    await pointer(stage.element, 'pointerdown', 20, 20, 37)
    stage.element.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, clientX: 30, clientY: 30 }))
    await nextTick()
    expect(onExport).not.toHaveBeenCalled()
    await pointer(stage.element, 'pointerup', 20, 20, 37)

    stage.element.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, clientX: 30, clientY: 30 }))
    await vi.waitFor(() => expect(onExport).toHaveBeenCalledTimes(1))
    expect(onExport.mock.calls[0]?.[0]).toBe('confirm')
  })

  it('uses right click to cancel an active gesture before cancelling capture', async () => {
    const { stage, onCancel } = await mountOverlay()
    await createSelection(stage)
    await pointer(stage.element, 'pointerdown', 20, 20, 17)
    stage.element.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 40 }))
    await nextTick()
    expect(onCancel).not.toHaveBeenCalled()

    stage.element.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 40 }))
    await nextTick()
    expect(onCancel).toHaveBeenCalledTimes(1)
  })

  it('uses right click to discard a text draft before cancelling capture', async () => {
    const { wrapper, stage, onCancel } = await mountOverlay()
    await createSelection(stage)
    await wrapper.get('[data-tool="text"]').trigger('click')
    await pointer(stage.element, 'pointerdown', 25, 25, 43)
    await pointer(stage.element, 'pointerup', 25, 25, 43)

    wrapper.get('[aria-label="Annotation text"]').element.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    await nextTick()
    expect(wrapper.find('[aria-label="Annotation text"]').exists()).toBe(false)
    expect(onCancel).not.toHaveBeenCalled()
  })

  it('changes the output selection without translating committed annotations', async () => {
    const onExport = vi.fn(async () => {})
    const { wrapper, stage } = await mountOverlay(onExport)
    await createSelection(stage)
    await wrapper.get('[data-tool="rect"]').trigger('click')
    await pointer(stage.element, 'pointerdown', 20, 20, 47)
    await pointer(stage.element, 'pointermove', 45, 40, 47)
    await pointer(stage.element, 'pointerup', 45, 40, 47)
    await wrapper.get('[data-tool="rect"]').trigger('click')

    await pointer(stage.element, 'pointerdown', 30, 30, 48)
    await pointer(stage.element, 'pointermove', 40, 35, 48)
    await pointer(stage.element, 'pointerup', 40, 35, 48)
    await wrapper.get('[data-action="copy"]').trigger('click')
    await vi.waitFor(() => expect(onExport).toHaveBeenCalledTimes(1))

    expect(onExport.mock.calls[0]?.[1].selection).toEqual({ x: 20, y: 15, width: 60, height: 50 })
    expect(wrapper.get('[aria-label="Undo"]').attributes()).not.toHaveProperty('disabled')
  })

  it('passes an exact clipped PNG and frame-pixel selection to retryable callbacks', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    const onExport = vi.fn().mockRejectedValueOnce(new Error('clipboard unavailable')).mockResolvedValueOnce(undefined)
    const { wrapper, stage } = await mountOverlay(onExport)
    await createSelection(stage, { x: 10, y: 15 }, { x: 60, y: 55 })
    await wrapper.get('[data-tool="rect"]').trigger('click')
    await pointer(stage.element, 'pointerdown', 20, 20, 41)
    await pointer(stage.element, 'pointermove', 40, 35, 41)
    await pointer(stage.element, 'pointerup', 40, 35, 41)

    await wrapper.get('[data-action="copy"]').trigger('click')
    await vi.waitFor(() => expect(wrapper.get('[role="alert"]').text()).toBe('Could not copy the capture. Try again.'))
    expect(wrapper.get('[data-testid="selection-dimensions"]').text()).toBe('50 × 40')
    expect(wrapper.get('[aria-label="Undo"]').attributes()).not.toHaveProperty('disabled')

    await wrapper.get('[data-action="copy"]').trigger('click')
    await vi.waitFor(() => expect(onExport).toHaveBeenCalledTimes(2))
    const [, payload] = onExport.mock.calls[1]!
    expect(payload.selection).toEqual({ x: 10, y: 15, width: 50, height: 40 })
    expect(payload.png.type).toBe('image/png')
    const bitmap = await createImageBitmap(payload.png)
    expect({ width: bitmap.width, height: bitmap.height }).toEqual({ width: 50, height: 40 })
    bitmap.close()
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
  })

  it('awaits an actually painted annotation frame and removes global listeners on dispose', async () => {
    const { wrapper, onCancel } = await mountOverlay()
    const overlay = wrapper.vm as unknown as { awaitPaint(): Promise<void>; dispose(): void }

    await expect(overlay.awaitPaint()).resolves.toBeUndefined()
    overlay.dispose()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await nextTick()
    expect(onCancel).not.toHaveBeenCalled()
  })
})
