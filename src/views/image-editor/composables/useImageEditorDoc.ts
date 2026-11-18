import * as Y from 'yjs'
import type { EditorLayer, CanvasSize, EditorTextLayer } from '../utils/types'
import {
  Y_META_KEY, Y_LAYERS_KEY, Y_IMAGES_KEY, DOCUMENT_FORMAT_VERSION, type MetaKey,
  createYLayer, readYLayer, moveYLayer, appendFreehandPoint, setYLayerProp,
} from '../doc/schema'

export interface ImageEditorDocOptions {
  gc?: boolean
}

export function useImageEditorDoc(options: ImageEditorDocOptions = {}) {
  const ydoc = new Y.Doc({ gc: options.gc ?? true })
  const meta = ydoc.getMap<unknown>(Y_META_KEY)
  meta.set('formatVersion', DOCUMENT_FORMAT_VERSION)
  const yLayers = ydoc.getArray<Y.Map<unknown>>(Y_LAYERS_KEY)
  const yImages = ydoc.getMap<string>(Y_IMAGES_KEY)

  function getMeta(key: MetaKey): unknown {
    return meta.get(key)
  }

  function setMeta(key: MetaKey, value: unknown): void {
    meta.set(key, value)
  }

  function getCanvasSize(): CanvasSize {
    return {
      width: (meta.get('canvasWidth') as number) ?? 1920,
      height: (meta.get('canvasHeight') as number) ?? 1080,
    }
  }

  function setCanvasSize(w: number, h: number): void {
    ydoc.transact(() => {
      meta.set('canvasWidth', w)
      meta.set('canvasHeight', h)
    })
  }

  function getBgColor(): string {
    return (meta.get('bgColor') as string) ?? 'transparent'
  }

  function setBgColor(color: string): void {
    meta.set('bgColor', color)
  }

  function getImgOffset(): { x: number; y: number } {
    return {
      x: (meta.get('imgOffsetX') as number) ?? 0,
      y: (meta.get('imgOffsetY') as number) ?? 0,
    }
  }

  function setImgOffset(x: number, y: number): void {
    ydoc.transact(() => {
      meta.set('imgOffsetX', x)
      meta.set('imgOffsetY', y)
    })
  }

  function getSourceImage(): string | null {
    return (meta.get('sourceImage') as string | null) ?? null
  }

  function setSourceImage(dataUrl: string | null): void {
    if (dataUrl) meta.set('sourceImage', dataUrl)
    else meta.delete('sourceImage')
  }

  function getImgAlpha(): number {
    return (meta.get('imgAlpha') as number) ?? 100
  }

  function setImgAlpha(alpha: number): void {
    meta.set('imgAlpha', alpha)
  }

  function getLayerCount(): number {
    return yLayers.length
  }

  function getLayers(): EditorLayer[] {
    return yLayers.map(m => readYLayer(m))
  }

  function getLayerYMap(id: string): Y.Map<unknown> | undefined {
    for (let i = 0; i < yLayers.length; i++) {
      const m = yLayers.get(i)
      if (m && m.get('id') === id) return m
    }
    return undefined
  }

  function getLayer(id: string): EditorLayer | undefined {
    const yMap = getLayerYMap(id)
    return yMap ? readYLayer(yMap) : undefined
  }

  function findLayerIndex(id: string): number {
    for (let i = 0; i < yLayers.length; i++) {
      const m = yLayers.get(i)
      if (m && m.get('id') === id) return i
    }
    return -1
  }

  function addLayer(layer: EditorLayer, index?: number): Y.Map<unknown> {
    const yMap = createYLayer(layer)
    if (index === undefined || index >= yLayers.length) {
      yLayers.push([yMap])
    } else if (index <= 0) {
      yLayers.unshift([yMap])
    } else {
      yLayers.insert(index, [yMap])
    }
    return yMap
  }

  function removeLayer(index: number): EditorLayer | undefined {
    if (index < 0 || index >= yLayers.length) return undefined
    const removed = readYLayer(yLayers.get(index))
    ydoc.transact(() => {
      yLayers.delete(index, 1)
      yImages.delete(removed.id)
    })
    return removed
  }

  function removeLayerById(id: string): EditorLayer | undefined {
    const idx = findLayerIndex(id)
    if (idx < 0) return undefined
    return removeLayer(idx)
  }

  function moveLayer(from: number, to: number): void {
    if (from === to) return
    if (from < 0 || from >= yLayers.length) return
    const target = to < 0 ? 0 : to >= yLayers.length ? yLayers.length - 1 : to
    const item = yLayers.get(from)
    if (!item) return
    const moved = item.clone()
    ydoc.transact(() => {
      yLayers.delete(from, 1)
      yLayers.insert(target, [moved])
    })
  }

  function setLayerProp(id: string, key: string, value: unknown): void {
    const yMap = getLayerYMap(id)
    if (!yMap) return
    if (value !== null && typeof value === 'object' && !Array.isArray(value) && !(value instanceof Y.Map)) {
      const newYMap = new Y.Map<unknown>()
      for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
        newYMap.set(k, v)
      }
      yMap.set(key, newYMap)
    } else {
      setYLayerProp(yMap, key, value)
    }
  }

  function setLayerProps(id: string, props: Record<string, unknown>): void {
    const yMap = getLayerYMap(id)
    if (!yMap) return
    ydoc.transact(() => {
      for (const [k, v] of Object.entries(props)) setYLayerProp(yMap, k, v)
    })
  }

  function setLayerMapProp<F extends 'textStroke' | 'textShadow', K extends keyof EditorTextLayer[F]>(id: string, field: F, key: K, value: EditorTextLayer[F][K]): void {
    const layer = getLayerYMap(id)
    if (!layer) return
    const map = layer.get(field) as Y.Map<unknown>
    map.set(key as string, value)
  }

  function moveLayerBy(id: string, dx: number, dy: number): void {
    const yMap = getLayerYMap(id)
    if (!yMap) return
    moveYLayer(yMap, dx, dy)
  }

  function appendPoint(id: string, x: number, y: number): void {
    const yMap = getLayerYMap(id)
    if (!yMap) return
    appendFreehandPoint(yMap, x, y)
  }

  function clearLayers(): void {
    ydoc.transact(() => {
      yLayers.delete(0, yLayers.length)
      yImages.clear()
    })
  }

  function getImageSrc(layerId: string): string | undefined {
    return yImages.get(layerId) ?? undefined
  }

  function setImageSrc(layerId: string, dataUrl: string): void {
    yImages.set(layerId, dataUrl)
  }

  function removeImageSrc(layerId: string): void {
    yImages.delete(layerId)
  }

  function getImageEntries(): Array<[string, string]> {
    return Array.from(yImages.entries())
  }

  function loadState(update: Uint8Array): void {
    const incoming = new Y.Doc()
    try {
      Y.applyUpdate(incoming, update)
      if (incoming.getMap(Y_META_KEY).get('formatVersion') !== DOCUMENT_FORMAT_VERSION) {
        throw new Error('Unsupported image editor document format')
      }
      for (const layer of incoming.getArray<Y.Map<unknown>>(Y_LAYERS_KEY).toArray()) readYLayer(layer)
    } finally { incoming.destroy() }
    Y.applyUpdate(ydoc, update, 'load')
  }

  function getStateUpdate(): Uint8Array {
    return Y.encodeStateAsUpdate(ydoc)
  }

  function getStateVector(): Uint8Array {
    return Y.encodeStateVector(ydoc)
  }

  function getDiffUpdate(remoteStateVector: Uint8Array): Uint8Array {
    return Y.encodeStateAsUpdate(ydoc, remoteStateVector)
  }

  function applyRemoteUpdate(update: Uint8Array): void {
    Y.applyUpdate(ydoc, update, 'remote')
  }

  function destroy(): void {
    ydoc.destroy()
  }

  return {
    ydoc,
    meta,
    yLayers,
    yImages,
    getMeta,
    setMeta,
    getCanvasSize,
    setCanvasSize,
    getBgColor,
    setBgColor,
    getImgOffset,
    setImgOffset,
    getSourceImage,
    setSourceImage,
    getImgAlpha,
    setImgAlpha,
    getLayerCount,
    getLayers,
    getLayerYMap,
    getLayer,
    findLayerIndex,
    addLayer,
    removeLayer,
    removeLayerById,
    moveLayer,
    setLayerProp,
    setLayerProps,
    setLayerMapProp,
    moveLayerBy,
    appendPoint,
    clearLayers,
    getImageSrc,
    setImageSrc,
    removeImageSrc,
    getImageEntries,
    loadState,
    getStateUpdate,
    getStateVector,
    getDiffUpdate,
    applyRemoteUpdate,
    destroy,
  }
}

export type ImageEditorDoc = ReturnType<typeof useImageEditorDoc>
