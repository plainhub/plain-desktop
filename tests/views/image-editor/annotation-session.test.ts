import { nextTick } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import annotationSessionSource from '@/views/image-editor/composables/useAnnotationSession.ts?raw'
import { useAnnotationSession } from '@/views/image-editor/composables/useAnnotationSession'

const externalEffects = vi.hoisted(() => ({
  gql: 0,
  store: 0,
  transport: 0,
  eventbus: 0,
  pixiSync: 0,
  pixiPaint: 0,
}))

vi.mock('@/lib/api/gql-client', () => ({
  gqlFetch: () => {
    externalEffects.gql++
    throw new Error('AnnotationSession attempted GraphQL')
  },
}))

vi.mock('@/plugins/eventbus', () => ({
  default: {
    on: () => {
      externalEffects.eventbus++
      throw new Error('AnnotationSession attempted eventbus subscription')
    },
    off: () => {},
    emit: () => {
      externalEffects.eventbus++
      throw new Error('AnnotationSession attempted eventbus publication')
    },
  },
}))

vi.mock('@/views/image-editor/store/plain-app-store', () => ({
  PlainAppProjectStore: class {
    constructor() {
      externalEffects.store++
      throw new Error('AnnotationSession constructed PlainAppProjectStore')
    }
  },
}))

vi.mock('@/views/image-editor/sync/event-sync-transport', () => ({
  EventSyncTransport: class {
    constructor() {
      externalEffects.transport++
      throw new Error('AnnotationSession constructed EventSyncTransport')
    }
  },
}))

vi.mock('@/views/image-editor/pixi/PixiEditorRenderer', () => ({
  PixiEditorRenderer: class {
    isReady = false
    async init() {
      this.isReady = true
    }
    resize() {}
    setViewport() {}
    sync() {
      externalEffects.pixiSync++
    }
    paint() {
      externalEffects.pixiPaint++
      return this.isReady
    }
    destroy() {
      this.isReady = false
    }
  },
}))

const sessions: Array<ReturnType<typeof useAnnotationSession>> = []

afterEach(() => {
  while (sessions.length) sessions.pop()!.dispose()
  externalEffects.gql = 0
  externalEffects.store = 0
  externalEffects.transport = 0
  externalEffects.eventbus = 0
  externalEffects.pixiSync = 0
  externalEffects.pixiPaint = 0
  vi.restoreAllMocks()
})

function createSession() {
  const session = useAnnotationSession()
  sessions.push(session)
  return session
}

function createCanvas(width: number, height: number): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  return canvas
}

function attachPointerCanvas(session: ReturnType<typeof useAnnotationSession>, width: number, height: number) {
  const canvas = createCanvas(width, height)
  canvas.getBoundingClientRect = () => ({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: width,
    bottom: height,
    width,
    height,
    toJSON: () => ({}),
  })
  session.canvasRef.value = canvas
  return canvas
}

function pointer(type: string, x: number, y: number): PointerEvent {
  return new PointerEvent(type, { button: 0, clientX: x, clientY: y })
}

describe('capture-safe AnnotationSession boundary', () => {
  it('has no static dependency on persistence, transport, router, network, or app state', () => {
    for (const forbidden of ['plain-app-store', 'event-sync-transport', 'useImageEditorPersistence', 'gql-client', '@/plugins/eventbus', '@/plugins/router', '@/stores/']) {
      expect(annotationSessionSource).not.toContain(forbidden)
    }
  })

  it('loads, mutates, exports, and disposes without persistence or navigation effects', async () => {
    const replaceState = vi.spyOn(window.history, 'replaceState')
    const release = vi.fn()
    const session = createSession()
    const source = createCanvas(12, 8)
    source.getContext('2d')!.fillRect(0, 0, 12, 8)

    session.source.set({ drawable: source, width: 12, height: 8, release })
    session.history.pushUndo()
    session.doc.addLayer({
      id: 'ephemeral-rect',
      type: 'rect',
      visible: true,
      name: 'Rect 1',
      x: 2,
      y: 2,
      w: 6,
      h: 4,
      color: '#ef4444',
      lineWidth: 2,
    })
    session.history.undo()
    session.history.redo()

    const rendered = await session.exportOps.renderPng({ x: 1, y: 1, width: 8, height: 5 })
    expect(rendered.ok).toBe(true)
    if (rendered.ok) expect(rendered.value.type).toBe('image/png')
    expect(session.doc.getSourceImage()).toBeNull()

    session.dispose()
    session.dispose()

    expect(release).toHaveBeenCalledTimes(1)
    expect(replaceState).not.toHaveBeenCalled()
    expect(externalEffects).toMatchObject({ gql: 0, store: 0, transport: 0, eventbus: 0 })
  })

  it('renders an exact clipped PNG without baking the transparency checkerboard', async () => {
    const session = createSession()
    const source = createCanvas(10, 8)
    const sourceContext = source.getContext('2d')!
    sourceContext.fillStyle = 'rgba(255, 0, 0, 1)'
    sourceContext.fillRect(4, 3, 2, 2)
    session.source.set({ drawable: source, width: 10, height: 8 })

    const rendered = await session.exportOps.renderPng({ x: 3, y: 2, width: 4, height: 4 })
    expect(rendered.ok).toBe(true)
    if (!rendered.ok) return

    const bitmap = await createImageBitmap(rendered.value)
    expect({ width: bitmap.width, height: bitmap.height }).toEqual({ width: 4, height: 4 })
    const output = createCanvas(4, 4)
    const outputContext = output.getContext('2d', { willReadFrequently: true })!
    outputContext.drawImage(bitmap, 0, 0)
    const pixels = outputContext.getImageData(0, 0, 4, 4)
    expect(pixels.data[3]).toBe(0)
    expect(Array.from(pixels.data.slice((1 * 4 + 1) * 4, (1 * 4 + 1) * 4 + 4))).toEqual([255, 0, 0, 255])
    bitmap.close()
  })

  it('returns an explicit error when PNG encoding produces no blob', async () => {
    const session = createSession()
    session.source.set({ drawable: createCanvas(5, 5), width: 5, height: 5 })
    vi.spyOn(HTMLCanvasElement.prototype, 'toBlob').mockImplementation((callback) => callback(null))

    await expect(session.exportOps.renderPng()).resolves.toEqual({ ok: false, error: 'encode-failed' })
  })

  it('keeps an in-flight export valid when the preview canvas reattaches', async () => {
    const session = createSession()
    session.source.set({ drawable: createCanvas(5, 5), width: 5, height: 5 })
    attachPointerCanvas(session, 5, 5)
    await nextTick()

    let finishEncoding!: BlobCallback
    vi.spyOn(HTMLCanvasElement.prototype, 'toBlob').mockImplementation((callback) => {
      finishEncoding = callback
    })

    const pendingExport = session.exportOps.renderPng()
    attachPointerCanvas(session, 5, 5)
    await nextTick()
    finishEncoding(new Blob(['png'], { type: 'image/png' }))

    await expect(pendingExport).resolves.toMatchObject({ ok: true })
  })

  it('validates decoded source dimensions before replacing the current source', () => {
    const session = createSession()
    const firstRelease = vi.fn()
    session.source.set({ drawable: createCanvas(8, 6), width: 8, height: 6, release: firstRelease })

    expect(() =>
      session.source.set({
        drawable: createCanvas(7, 6),
        width: 8,
        height: 6,
      })
    ).toThrowError('Annotation source dimensions must match its decoded raster')
    expect(firstRelease).not.toHaveBeenCalled()
    expect(session.state.sourceImg.value).toHaveProperty('width', 8)
  })

  it('finishes source replacement and disposal even when release callbacks throw', () => {
    const session = createSession()
    const throwingRelease = vi.fn(() => {
      throw new Error('release failed')
    })
    const finalRelease = vi.fn()

    session.source.set({ drawable: createCanvas(8, 6), width: 8, height: 6, release: throwingRelease })
    expect(() =>
      session.source.set({
        drawable: createCanvas(4, 3),
        width: 4,
        height: 3,
        release: finalRelease,
      })
    ).not.toThrow()
    expect(throwingRelease).toHaveBeenCalledTimes(1)
    expect(session.state.canvasSize.value).toEqual({ width: 4, height: 3 })

    expect(() => session.dispose()).not.toThrow()
    session.dispose()
    expect(finalRelease).toHaveBeenCalledTimes(1)
    expect(session.render.preview()).toEqual({ ok: false, error: 'disposed' })
  })

  it('groups a multi-move layer drag into one undo item and invalidates redo', async () => {
    const session = createSession()
    session.source.set({ drawable: createCanvas(100, 100), width: 100, height: 100 })
    attachPointerCanvas(session, 100, 100)
    await nextTick()

    session.tools.activeTool.value = 'rect'
    session.pointer.onPointerDown(pointer('pointerdown', 10, 10))
    session.pointer.onPointerMove(pointer('pointermove', 80, 80))
    session.pointer.onPointerUp()
    expect(session.state.layers[0]).toMatchObject({ x: 10, y: 10, w: 70, h: 70 })

    session.tools.activeTool.value = 'select'
    session.pointer.onPointerDown(pointer('pointerdown', 45, 45))
    session.pointer.onPointerMove(pointer('pointermove', 50, 50))
    session.pointer.onPointerMove(pointer('pointermove', 60, 55))
    session.pointer.onPointerMove(pointer('pointermove', 70, 60))
    session.pointer.onPointerUp()
    expect(session.state.layers[0]).toMatchObject({ x: 35, y: 25 })

    session.history.undo()
    expect(session.state.layers[0]).toMatchObject({ x: 10, y: 10 })
    expect(session.history.canRedo.value).toBe(true)

    session.history.pushUndo()
    session.doc.setBgColor('#ffffff')
    expect(session.history.canRedo.value).toBe(false)
  })

  it('does not collapse unrelated document mutations into the previous gesture', () => {
    const session = createSession()
    session.source.set({ drawable: createCanvas(10, 10), width: 10, height: 10 })

    session.doc.setBgColor('#ffffff')
    session.doc.setImgAlpha(40)
    session.history.undo()

    expect(session.state.imgAlpha.value).toBe(100)
    expect(session.state.bgColor.value).toBe('#ffffff')
    session.history.undo()
    expect(session.state.bgColor.value).toBe('transparent')
  })

  it('commits a multi-move layer transform as one undo item', async () => {
    const session = createSession()
    session.source.set({ drawable: createCanvas(100, 100), width: 100, height: 100 })
    attachPointerCanvas(session, 100, 100)
    session.doc.addLayer({
      id: 'scaled-rect',
      type: 'rect',
      visible: true,
      name: 'Rect 1',
      x: 10,
      y: 10,
      w: 40,
      h: 40,
      color: '#ef4444',
      lineWidth: 4,
    })
    session.state.selectedLayerId.value = 'scaled-rect'
    session.history.clearHistory()
    await nextTick()

    session.pointer.onPointerDown(pointer('pointerdown', 60, 60))
    session.pointer.onPointerMove(pointer('pointermove', 70, 70))
    session.pointer.onPointerMove(pointer('pointermove', 80, 80))
    session.pointer.onPointerUp()
    expect(session.state.layers[0]).toMatchObject({ w: 67, h: 67 })

    session.history.undo()
    expect(session.state.layers[0]).toMatchObject({ x: 10, y: 10, w: 40, h: 40 })
    expect(session.history.canUndo.value).toBe(false)
  })

  it('rolls back canceled layer moves and transforms without retaining history', async () => {
    const session = createSession()
    session.source.set({ drawable: createCanvas(100, 100), width: 100, height: 100 })
    attachPointerCanvas(session, 100, 100)
    session.doc.addLayer({
      id: 'cancel-rect',
      type: 'rect',
      visible: true,
      name: 'Rect 1',
      x: 10,
      y: 10,
      w: 40,
      h: 40,
      color: '#ef4444',
      lineWidth: 4,
    })
    session.state.selectedLayerId.value = 'cancel-rect'
    session.history.clearHistory()
    session.doc.setBgColor('#ffffff')
    session.history.undo()
    expect(session.history.canRedo.value).toBe(true)
    await nextTick()

    session.pointer.onPointerDown(pointer('pointerdown', 30, 30))
    session.pointer.onPointerMove(pointer('pointermove', 50, 45))
    expect(session.state.layers[0]).toMatchObject({ x: 30, y: 25 })
    session.pointer.onPointerCancel()
    expect(session.state.layers[0]).toMatchObject({ x: 10, y: 10, w: 40, h: 40 })
    expect(session.history.canUndo.value).toBe(false)
    expect(session.history.canRedo.value).toBe(true)
    session.history.redo()
    expect(session.state.bgColor.value).toBe('#ffffff')
    session.history.clearHistory()

    session.state.selectedLayerId.value = 'cancel-rect'
    session.pointer.onPointerDown(pointer('pointerdown', 60, 60))
    session.pointer.onPointerMove(pointer('pointermove', 80, 80))
    expect(session.state.layers[0]).toMatchObject({ w: 67, h: 67 })
    session.pointer.onPointerCancel()
    expect(session.state.layers[0]).toMatchObject({ x: 10, y: 10, w: 40, h: 40 })
    expect(session.history.canUndo.value).toBe(false)
    expect(session.history.canRedo.value).toBe(false)
  })

  it('resolves its paint barrier only after the requested preview flushes', async () => {
    const session = createSession()
    session.source.set({ drawable: createCanvas(20, 20), width: 20, height: 20 })
    attachPointerCanvas(session, 20, 20)
    await nextTick()
    await new Promise(requestAnimationFrame)
    const priorPaints = externalEffects.pixiSync

    const result = await session.render.awaitPaint()

    expect(result).toEqual({ ok: true, value: undefined })
    expect(externalEffects.pixiSync).toBe(priorPaints + 1)
    expect(externalEffects.pixiPaint).toBe(1)
  })

  it('keeps capture text as a draft until commit and cancels it without a phantom layer', async () => {
    const session = createSession()
    session.source.set({ drawable: createCanvas(100, 100), width: 100, height: 100 })
    attachPointerCanvas(session, 100, 100)
    await nextTick()

    session.tools.activeTool.value = 'text'
    session.pointer.onPointerDown(pointer('pointerdown', 30, 40))
    expect(session.state.layers).toHaveLength(0)
    expect(session.text.draft.value).toMatchObject({ type: 'text', x: 30, y: 40, text: '' })

    session.text.update('capture label')
    expect(session.state.layers).toHaveLength(0)
    expect(session.text.commit()).toMatchObject({ ok: true })
    expect(session.state.layers).toHaveLength(1)
    expect(session.state.layers[0]).toMatchObject({ type: 'text', text: 'capture label' })
    session.history.undo()
    expect(session.state.layers).toHaveLength(0)

    session.tools.activeTool.value = 'text'
    session.pointer.onPointerDown(pointer('pointerdown', 50, 60))
    session.text.update('discard me')
    session.text.cancel()
    expect(session.text.draft.value).toBeNull()
    expect(session.state.layers).toHaveLength(0)
    expect(session.history.canUndo.value).toBe(false)
  })

  it('makes preview readiness, gesture cancellation, and disposal explicit', async () => {
    const session = createSession()
    session.source.set({ drawable: createCanvas(40, 40), width: 40, height: 40 })
    expect(session.render.preview()).toEqual({ ok: false, error: 'preview-not-ready' })

    attachPointerCanvas(session, 40, 40)
    await nextTick()
    expect(session.render.preview()).toEqual({ ok: true, value: undefined })

    session.tools.activeTool.value = 'rect'
    session.pointer.onPointerDown(pointer('pointerdown', 4, 4))
    session.pointer.onPointerMove(pointer('pointermove', 20, 20))
    session.pointer.onPointerCancel()
    expect(session.state.layers).toHaveLength(0)

    const pendingPaint = session.render.awaitPaint()
    session.dispose()
    expect(session.render.preview()).toEqual({ ok: false, error: 'disposed' })
    await expect(pendingPaint).resolves.toEqual({ ok: false, error: 'disposed' })
    await expect(session.render.awaitPaint()).resolves.toEqual({ ok: false, error: 'disposed' })
    await expect(session.exportOps.renderPng()).resolves.toEqual({ ok: false, error: 'disposed' })
  })
})
