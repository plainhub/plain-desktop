import { ref, reactive, shallowRef, type Ref, type Reactive } from 'vue'
import * as Y from 'yjs'
import type { EditorLayer, CanvasSize, EditorRasterSource } from '../utils/types'
import type { ImageEditorDoc } from './useImageEditorDoc'

export interface ImageEditorDocBinding {
  canvasSize: Ref<CanvasSize>
  bgColor: Ref<string>
  imgOffset: Reactive<{ x: number; y: number }>
  imgAlpha: Ref<number>
  sourceImg: Ref<EditorRasterSource | null>
  layers: Reactive<EditorLayer[]>
  layerImages: Map<string, HTMLImageElement>
  layerImageVersions: Map<string, number>
  syncVersion: Ref<number>
  dispose: () => void
}

export function useImageEditorDocBinding(doc: ImageEditorDoc): ImageEditorDocBinding {
  const canvasSize = ref<CanvasSize>(doc.getCanvasSize())
  const bgColor = ref(doc.getBgColor())
  const imgOffset = reactive(doc.getImgOffset())
  const imgAlpha = ref(doc.getImgAlpha())
  const sourceImg = shallowRef<EditorRasterSource | null>(null)
  const layers = reactive<EditorLayer[]>([]) as Reactive<EditorLayer[]>
  const layerImages = new Map<string, HTMLImageElement>()
  const layerImageVersions = new Map<string, number>()
  const syncVersion = ref(0)

  let disposed = false

  function rebuildLayers(): void {
    const fresh = doc.getLayers()
    if (layers.length !== fresh.length) {
      layers.splice(0, layers.length, ...fresh)
      return
    }
    for (let i = 0; i < fresh.length; i++) {
      const existing = layers[i]
      const next = fresh[i]!
      if (!existing || existing.id !== next.id) {
        layers.splice(i, 1, next)
      } else {
        layers.splice(i, 1, { ...next })
      }
    }
  }

  function rebuildSourceImage(): void {
    const dataUrl = doc.getSourceImage()
    if (!dataUrl) {
      sourceImg.value = null
      return
    }
    const img = new Image()
    img.onload = () => {
      if (disposed) return
      if (doc.getSourceImage() === dataUrl) sourceImg.value = img
    }
    img.src = dataUrl
  }

  function rebuildLayerImage(layerId: string): void {
    const dataUrl = doc.getImageSrc(layerId)
    if (!dataUrl) {
      layerImages.delete(layerId)
      layerImageVersions.delete(layerId)
      return
    }
    const img = new Image()
    const version = (layerImageVersions.get(layerId) ?? 0) + 1
    layerImageVersions.set(layerId, version)
    img.onload = () => {
      if (disposed) return
      if (layerImageVersions.get(layerId) !== version) return
      if (doc.getImageSrc(layerId) === dataUrl) {
        layerImages.set(layerId, img)
        syncVersion.value++
      }
    }
    img.src = dataUrl
  }

  function rebuildAllLayerImages(): void {
    const seen = new Set<string>()
    for (const [layerId] of doc.getImageEntries()) {
      seen.add(layerId)
      if (!layerImages.has(layerId) || doc.getImageSrc(layerId) !== layerImages.get(layerId)?.src) {
        rebuildLayerImage(layerId)
      }
    }
    for (const layerId of Array.from(layerImages.keys())) {
      if (!seen.has(layerId)) {
        layerImages.delete(layerId)
        layerImageVersions.delete(layerId)
      }
    }
  }

  function syncAll(): void {
    canvasSize.value = doc.getCanvasSize()
    bgColor.value = doc.getBgColor()
    const offset = doc.getImgOffset()
    imgOffset.x = offset.x
    imgOffset.y = offset.y
    imgAlpha.value = doc.getImgAlpha()
    rebuildLayers()
    rebuildSourceImage()
    rebuildAllLayerImages()
    syncVersion.value++
  }

  function onMetaUpdate(event: Y.YMapEvent<unknown>): void {
    if (disposed) return
    for (const key of event.keysChanged) {
      switch (key) {
        case 'canvasWidth':
        case 'canvasHeight':
          canvasSize.value = doc.getCanvasSize()
          break
        case 'bgColor':
          bgColor.value = doc.getBgColor()
          break
        case 'imgOffsetX':
        case 'imgOffsetY': {
          const offset = doc.getImgOffset()
          imgOffset.x = offset.x
          imgOffset.y = offset.y
          break
        }
        case 'imgAlpha':
          imgAlpha.value = doc.getImgAlpha()
          break
        case 'sourceImage':
          rebuildSourceImage()
          break
      }
    }
  }

  function onLayerMapUpdate(): void {
    if (disposed) return
    rebuildLayers()
  }

  function onImagesUpdate(event: Y.YMapEvent<string>): void {
    if (disposed) return
    for (const [layerId, change] of event.keys) {
      if (change.action === 'delete') {
        layerImages.delete(layerId)
        layerImageVersions.delete(layerId)
      } else {
        rebuildLayerImage(layerId)
      }
    }
  }

  function onDocUpdate(): void {
    if (disposed) return
    syncVersion.value++
  }

  const layerObserverMap = new Map<Y.Map<unknown>, (event: Y.YMapEvent<unknown>) => void>()

  function attachLayerObservers(): void {
    for (let i = 0; i < doc.yLayers.length; i++) {
      const yMap = doc.yLayers.get(i)
      if (!yMap || layerObserverMap.has(yMap)) continue
      const handler = () => onLayerMapUpdate()
      yMap.observe(handler)
      layerObserverMap.set(yMap, handler)
    }
    for (const [yMap, handler] of layerObserverMap) {
      let stillExists = false
      for (let i = 0; i < doc.yLayers.length; i++) {
        if (doc.yLayers.get(i) === yMap) { stillExists = true; break }
      }
      if (!stillExists) {
        yMap.unobserve(handler)
        layerObserverMap.delete(yMap)
      }
    }
  }

  function onLayersUpdateWithObservers(event: Y.YArrayEvent<Y.Map<unknown>>): void {
    if (disposed) return
    attachLayerObservers()
    if (event.changes.delta.length === 0) return
    rebuildLayers()
  }

  const metaHandler = (event: Y.YMapEvent<unknown>) => onMetaUpdate(event)
  const layersHandler = (event: Y.YArrayEvent<Y.Map<unknown>>) => onLayersUpdateWithObservers(event)
  const imagesHandler = (event: Y.YMapEvent<string>) => onImagesUpdate(event)
  const docHandler = () => onDocUpdate()

  doc.meta.observe(metaHandler)
  doc.yLayers.observe(layersHandler)
  doc.yImages.observe(imagesHandler)
  doc.ydoc.on('update', docHandler)

  syncAll()
  attachLayerObservers()

  function dispose(): void {
    if (disposed) return
    disposed = true
    doc.meta.unobserve(metaHandler)
    doc.yLayers.unobserve(layersHandler)
    doc.yImages.unobserve(imagesHandler)
    doc.ydoc.off('update', docHandler)
    for (const [yMap, handler] of layerObserverMap) {
      yMap.unobserve(handler)
    }
    layerObserverMap.clear()
    layerImages.clear()
    layerImageVersions.clear()
  }

  return {
    canvasSize,
    bgColor,
    imgOffset,
    imgAlpha,
    sourceImg,
    layers,
    layerImages,
    layerImageVersions,
    syncVersion,
    dispose,
  }
}
