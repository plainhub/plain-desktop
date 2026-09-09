import { Application, Container, Graphics, Sprite, Texture, TilingSprite } from 'pixi.js'
import type {
  EditorLayer, CanvasSize, ArrowLayer, RectLayer, EllipseLayer, HighlightLayer,
  FreehandLayer, EditorTextLayer, EditorImageLayer, StickerLayer, MosaicLayer, EditorRasterSource,
} from '../utils/types'
import { getEditorLayerBounds, getEditorRasterSize } from '../utils/types'
import { drawLayer, wrapText } from '../utils/editor-draw-layers'

export interface PixiRenderState {
  canvasSize: CanvasSize
  bgColor: string
  sourceImg: EditorRasterSource | null
  imgOffset: { x: number; y: number }
  imgAlpha: number
  layers: EditorLayer[]
  layerImages: Map<string, HTMLImageElement>
  previewLayer: EditorLayer | null
  hideLayerId: string | null
}

const PAD = 60

function colorToNumber(c: string): number | null {
  if (c.startsWith('#') && c.length === 7) {
    return parseInt(c.slice(1), 16)
  }
  return null
}

function parseGradientColors(value: string): { angle: number; colors: string[] } | null {
  if (value.startsWith('linear-gradient')) {
    const inner = value.replace(/^linear-gradient\(/, '').replace(/\)$/, '')
    const parts = inner.split(',')
    const first = parts[0]!.trim()
    if (first.endsWith('deg')) {
      return { angle: parseFloat(first), colors: parts.slice(1).map(s => s.trim()).filter(Boolean) }
    }
    return { angle: 0, colors: parts.map(s => s.trim()).filter(Boolean) }
  }
  if (value.startsWith('gradient:')) {
    const parts = value.split(':')
    return { angle: parseFloat(parts[2] ?? '0'), colors: [parts[3] ?? '#000000', parts[4] ?? '#ffffff'] }
  }
  return null
}

function makeCheckerboardTexture(): Texture {
  const c = document.createElement('canvas')
  c.width = 32; c.height = 32
  const ctx = c.getContext('2d')!
  ctx.fillStyle = '#f8f8f8'; ctx.fillRect(0, 0, 32, 32)
  ctx.fillStyle = '#e5e5e5'
  ctx.fillRect(0, 0, 16, 16)
  ctx.fillRect(16, 16, 16, 16)
  return Texture.from(c)
}

function makeGradientTexture(value: string, w: number, h: number): Texture {
  const parsed = parseGradientColors(value)
  const angle = parsed?.angle ?? 0
  const colors = parsed?.colors ?? ['#000000', '#ffffff']
  const c = document.createElement('canvas')
  c.width = Math.max(1, w); c.height = Math.max(1, h)
  const ctx = c.getContext('2d')!
  const rad = ((angle - 90) * Math.PI) / 180
  const cx = w / 2, cy = h / 2
  const diag = Math.sqrt(w * w + h * h) / 2
  const dx = Math.cos(rad) * diag, dy = Math.sin(rad) * diag
  const grad = ctx.createLinearGradient(cx - dx, cy - dy, cx + dx, cy + dy)
  for (let i = 0; i < colors.length; i++) {
    grad.addColorStop(i / Math.max(1, colors.length - 1), colors[i]!)
  }
  ctx.fillStyle = grad
  ctx.fillRect(0, 0, w, h)
  return Texture.from(c)
}

function textLayerHash(l: EditorTextLayer): string {
  return JSON.stringify({
    t: l.text, fs: l.fontSize, c: l.color, ff: l.fontFamily,
    fw: l.fontWeight, fst: l.fontStyle, ta: l.textAlign,
    ls: l.letterSpacing, td: l.textDecoration,
    ts: l.textStroke, tsh: l.textShadow, g: l.glow,
    mw: l.maxWidth, r: l.rotation,
  })
}

function stickerLayerHash(l: StickerLayer): string {
  return JSON.stringify({
    t: l.text, fs: l.fontSize, c: l.color, fw: l.fontWeight,
    fst: l.fontStyle, w: l.w, h: l.h, r: l.rotation,
  })
}

function renderTextToCanvas(l: EditorTextLayer): { canvas: HTMLCanvasElement; cx: number; cy: number } {
  const padX = Math.max(PAD, (l.textShadow.blur || 0) + Math.abs(l.textShadow.offsetX) + 20)
  const padY = Math.max(PAD, (l.textShadow.blur || 0) + Math.abs(l.textShadow.offsetY) + 20)
  const w = l.maxWidth + padX * 2
  const fontStyle = l.fontStyle === 'italic' ? 'italic ' : ''
  const fontWeight = l.fontWeight === 'bold' ? 'bold ' : ''
  const ctx = document.createElement('canvas').getContext('2d')!
  ctx.font = `${fontStyle}${fontWeight}${l.fontSize}px ${l.fontFamily}`
  const lines = wrapText(ctx, l.text, l.maxWidth)
  const lineH = l.fontSize * 1.15
  const textH = lines.length * lineH
  const h = textH + padY * 2

  const canvas = document.createElement('canvas')
  canvas.width = Math.ceil(w); canvas.height = Math.ceil(h)
  const c2 = canvas.getContext('2d')!
  c2.translate(padX + l.maxWidth / 2 - l.x, padY + textH / 2 - l.y)
  drawLayer(c2, l)
  return { canvas, cx: padX + l.x, cy: padY + l.y }
}

function renderStickerToCanvas(l: StickerLayer): { canvas: HTMLCanvasElement; cx: number; cy: number } {
  const pad = PAD
  const w = l.w + pad * 2
  const h = l.h + pad * 2
  const canvas = document.createElement('canvas')
  canvas.width = Math.ceil(w); canvas.height = Math.ceil(h)
  const ctx = canvas.getContext('2d')!
  ctx.translate(pad + l.w / 2 - l.x, pad + l.h / 2 - l.y)
  drawLayer(ctx, l)
  return { canvas, cx: pad + l.x, cy: pad + l.y }
}

function isGradientBg(v: string): boolean {
  return v.startsWith('gradient:') || v.startsWith('linear-gradient') || v.startsWith('radial-gradient')
}

function layerContentHash(l: EditorLayer, state: PixiRenderState): string {
  switch (l.type) {
    case 'arrow':
      return `arrow|${l.x1},${l.y1},${l.x2},${l.y2}|${l.color}|${l.lineWidth}|${l.rotation ?? 0}`
    case 'rect':
      return `rect|${l.x},${l.y},${l.w},${l.h}|${l.color}|${l.lineWidth}`
    case 'ellipse':
      return `ellipse|${l.cx},${l.cy},${l.rx},${l.ry}|${l.color}|${l.lineWidth}`
    case 'highlight':
      return `highlight|${l.x},${l.y},${l.w},${l.h}|${l.color}`
    case 'freehand': {
      const p = l.points
      const n = p.length
      const head = p[0]
      const tail = p[n - 1]
      return `freehand|${n}|${head?.x ?? 0},${head?.y ?? 0}|${tail?.x ?? 0},${tail?.y ?? 0}|${l.color}|${l.lineWidth}`
    }
    case 'text':
      return `text|${textLayerHash(l)}`
    case 'sticker':
      return `sticker|${stickerLayerHash(l)}`
    case 'image': {
      const img = state.layerImages.get(l.id)
      return `image|${l.x},${l.y},${l.w},${l.h}|${l.opacity}|${l.rotation}|${img?.src ?? ''}`
    }
    case 'mosaic': {
      const sourceSize = state.sourceImg ? getEditorRasterSize(state.sourceImg) : { width: 0, height: 0 }
      return `mosaic|${Math.round(l.x)},${Math.round(l.y)},${Math.round(l.w)},${Math.round(l.h)}|${l.blockSize}|${state.imgOffset.x},${state.imgOffset.y}|${sourceSize.width},${sourceSize.height}`
    }
    default:
      return Math.random().toString(36)
  }
}

export class PixiEditorRenderer {
  private app: Application | null = null
  private worldContainer: Container | null = null
  private layerContainer: Container | null = null
  private checkerboard: TilingSprite | null = null
  private bgRect: Graphics | null = null
  private bgGradient: Sprite | null = null
  private bgImage: Sprite | null = null
  private layerNodes = new Map<string, Container>()
  private layerHashes = new Map<string, string>()
  private textTextureCache = new Map<string, { texture: Texture; hash: string }>()
  private mosaicTextureCache = new Map<string, { texture: Texture; hash: string }>()
  private state: PixiRenderState | null = null
  private checkerTexture: Texture | null = null
  private ready = false

  private lastCanvasW = 0
  private lastCanvasH = 0
  private lastBgColor: string | null = null
  private bgGradientTexture: Texture | null = null
  private lastBgGradientKey: string | null = null
  private bgImageTexture: Texture | null = null
  private lastBgImageSource: EditorRasterSource | null = null
  private lifecycleGeneration = 0

  get isReady(): boolean { return this.ready }

  async init(canvas: HTMLCanvasElement): Promise<void> {
    if (this.ready) this.destroy()

    const generation = ++this.lifecycleGeneration
    const app = new Application()
    await app.init({
      canvas,
      background: 0xffffff,
      antialias: true,
      autoDensity: false,
      resolution: 1,
      width: canvas.clientWidth || 1920,
      height: canvas.clientHeight || 1080,
      preference: 'webgl',
    })
    if (generation !== this.lifecycleGeneration) {
      app.destroy(true)
      return
    }
    this.app = app

    this.worldContainer = new Container()
    this.layerContainer = new Container()
    app.stage.addChild(this.worldContainer)

    this.checkerTexture = makeCheckerboardTexture()
    this.checkerboard = new TilingSprite({
      texture: this.checkerTexture,
      width: 1920,
      height: 1080,
    })
    this.bgRect = new Graphics()
    this.bgGradient = new Sprite(Texture.EMPTY)
    this.bgGradient.visible = false
    this.bgImage = new Sprite(Texture.EMPTY)
    this.bgImage.visible = false

    this.worldContainer.addChild(this.checkerboard)
    this.worldContainer.addChild(this.bgRect)
    this.worldContainer.addChild(this.bgGradient)
    this.worldContainer.addChild(this.bgImage)
    this.worldContainer.addChild(this.layerContainer)

    canvas.style.width = '100%'
    canvas.style.height = '100%'
    canvas.style.display = 'block'

    this.ready = true
  }

  setViewport(scale: number, panX: number, panY: number): void {
    if (!this.worldContainer) return
    this.worldContainer.scale.set(scale)
    this.worldContainer.position.set(panX, panY)
  }

  resize(width: number, height: number): void {
    if (!this.app) return
    this.app.renderer.resize(width, height)
    if (this.app.canvas) {
      this.app.canvas.style.width = '100%'
      this.app.canvas.style.height = '100%'
    }
  }

  sync(state: PixiRenderState): void {
    if (!this.ready || !this.worldContainer || !this.layerContainer) return
    this.state = state
    const { width: cw, height: ch } = state.canvasSize

    this.syncCheckerboard(cw, ch)
    this.syncBackground(state, cw, ch)
    this.syncLayers(state)

    this.lastCanvasW = cw
    this.lastCanvasH = ch
  }

  paint(): boolean {
    if (!this.ready || !this.app) return false
    this.app.render()
    return true
  }

  private syncCheckerboard(cw: number, ch: number): void {
    if (!this.checkerboard) return
    if (this.checkerboard.width !== cw || this.checkerboard.height !== ch) {
      this.checkerboard.width = cw
      this.checkerboard.height = ch
    }
  }

  private syncBackground(state: PixiRenderState, cw: number, ch: number): void {
    const { bgColor, sourceImg, imgOffset, imgAlpha } = state
    const sizeChanged = this.lastCanvasW !== cw || this.lastCanvasH !== ch

    if (this.bgRect) {
      const needSolid = bgColor !== 'transparent' && !isGradientBg(bgColor)
      if (needSolid && (this.lastBgColor !== bgColor || sizeChanged)) {
        this.bgRect.clear()
        this.bgRect.rect(0, 0, cw, ch).fill(bgColor)
        this.lastBgColor = bgColor
      } else if (!needSolid && this.lastBgColor !== null) {
        this.bgRect.clear()
        this.lastBgColor = null
      }
      this.bgRect.visible = needSolid
    }

    if (this.bgGradient) {
      const isGrad = bgColor !== 'transparent' && (bgColor.startsWith('gradient:') || bgColor.startsWith('linear-gradient'))
      if (isGrad) {
        const key = `${bgColor}|${cw}x${ch}`
        if (this.lastBgGradientKey !== key) {
          this.bgGradientTexture?.destroy(true)
          this.bgGradientTexture = makeGradientTexture(bgColor, cw, ch)
          this.bgGradient.texture = this.bgGradientTexture
          this.lastBgGradientKey = key
        }
      }
      this.bgGradient.visible = isGrad
    }

    if (this.bgImage) {
      if (sourceImg) {
        if (this.lastBgImageSource !== sourceImg) {
          this.bgImageTexture?.destroy(true)
          this.bgImageTexture = Texture.from(sourceImg)
          this.bgImage.texture = this.bgImageTexture
          this.lastBgImageSource = sourceImg
        }
        this.bgImage.position.set(imgOffset.x, imgOffset.y)
        this.bgImage.alpha = imgAlpha / 100
      }
      this.bgImage.visible = !!sourceImg
    }
  }

  private syncLayers(state: PixiRenderState): void {
    if (!this.layerContainer) return
    const allLayers = state.previewLayer
      ? [...state.layers, state.previewLayer]
      : state.layers
    const visibleLayers = allLayers.filter(l => l.visible && l.id !== state.hideLayerId)
    const seen = new Set<string>()

    for (const layer of visibleLayers) {
      seen.add(layer.id)
      const hash = layerContentHash(layer, state)
      const existing = this.layerNodes.get(layer.id)
      if (!existing) {
        const node = new Container()
        this.layerContainer.addChild(node)
        this.layerNodes.set(layer.id, node)
        this.updateLayerNode(node, layer, state)
        this.layerHashes.set(layer.id, hash)
      } else {
        const prevHash = this.layerHashes.get(layer.id)
        if (prevHash !== hash) {
          this.updateLayerNode(existing, layer, state)
          this.layerHashes.set(layer.id, hash)
        }
      }
    }

    for (const [id, node] of this.layerNodes) {
      if (!seen.has(id)) {
        this.layerContainer.removeChild(node)
        this.destroyLayerNode(node, id)
        this.layerNodes.delete(id)
        this.layerHashes.delete(id)
      }
    }

    for (let i = 0; i < visibleLayers.length; i++) {
      const node = this.layerNodes.get(visibleLayers[i]!.id)
      if (node) this.layerContainer.setChildIndex(node, i)
    }
  }

  private updateLayerNode(node: Container, layer: EditorLayer, state: PixiRenderState): void {
    node.pivot.set(0, 0)
    node.position.set(0, 0)
    node.rotation = 0

    switch (layer.type) {
      case 'arrow': {
        const g = this.reuseGraphics(node)
        this.drawArrowLayer(g, layer)
        this.setRotation(node, (layer.x1 + layer.x2) / 2, (layer.y1 + layer.y2) / 2, layer.rotation ?? 0)
        break
      }
      case 'rect': this.drawRectLayer(this.reuseGraphics(node), layer); break
      case 'ellipse': this.drawEllipseLayer(this.reuseGraphics(node), layer); break
      case 'highlight': this.drawHighlightLayer(this.reuseGraphics(node), layer); break
      case 'freehand': this.drawFreehandLayer(this.reuseGraphics(node), layer); break
      case 'text': this.clearNodeChildren(node); this.drawTextLayer(node, layer); break
      case 'sticker': this.clearNodeChildren(node); this.drawStickerLayer(node, layer); break
      case 'image': this.clearNodeChildren(node); this.drawImageLayer(node, layer, state.layerImages); break
      case 'mosaic': this.clearNodeChildren(node); this.drawMosaicLayer(node, layer); break
    }
  }

  private reuseGraphics(node: Container): Graphics {
    const existing = node.children[0]
    if (existing instanceof Graphics) return existing
    node.removeChildren().forEach(c => c.destroy())
    const g = new Graphics()
    node.addChild(g)
    return g
  }

  private clearNodeChildren(node: Container): void {
    node.removeChildren().forEach(c => c.destroy())
  }

  private setRotation(node: Container, cx: number, cy: number, rotation: number): void {
    if (rotation) {
      node.pivot.set(cx, cy)
      node.position.set(cx, cy)
      node.rotation = (rotation * Math.PI) / 180
    }
  }

  private drawArrowLayer(g: Graphics, l: ArrowLayer): void {
    g.clear()
    const angle = Math.atan2(l.y2 - l.y1, l.x2 - l.x1)
    const headLen = Math.max(l.lineWidth * 4, 12)
    const headW = headLen * 0.5
    const baseX = l.x2 - headLen * Math.cos(angle)
    const baseY = l.y2 - headLen * Math.sin(angle)

    g.moveTo(l.x1, l.y1).lineTo(baseX, baseY)
    g.stroke({ color: l.color, width: l.lineWidth, cap: 'round', join: 'round' })
    g.moveTo(l.x2, l.y2)
      .lineTo(baseX + headW * Math.sin(angle), baseY - headW * Math.cos(angle))
      .lineTo(baseX - headW * Math.sin(angle), baseY + headW * Math.cos(angle))
      .closePath()
    g.fill(l.color)
  }

  private drawRectLayer(g: Graphics, l: RectLayer): void {
    g.clear()
    g.rect(l.x, l.y, l.w, l.h)
    g.stroke({ color: l.color, width: l.lineWidth, cap: 'round', join: 'round' })
  }

  private drawEllipseLayer(g: Graphics, l: EllipseLayer): void {
    g.clear()
    if (l.rx > 0 && l.ry > 0) {
      g.ellipse(l.cx, l.cy, l.rx, l.ry)
      g.stroke({ color: l.color, width: l.lineWidth })
    }
  }

  private drawHighlightLayer(g: Graphics, l: HighlightLayer): void {
    g.clear()
    g.rect(l.x, l.y, l.w, l.h)
    const num = colorToNumber(l.color)
    g.fill(num != null ? num : l.color)
    g.alpha = 0.35
  }

  private drawFreehandLayer(g: Graphics, l: FreehandLayer): void {
    g.clear()
    if (l.points.length < 2) return
    g.moveTo(l.points[0]!.x, l.points[0]!.y)
    for (let i = 1; i < l.points.length - 1; i++) {
      const curr = l.points[i]!
      const next = l.points[i + 1]!
      const midX = (curr.x + next.x) / 2
      const midY = (curr.y + next.y) / 2
      g.quadraticCurveTo(curr.x, curr.y, midX, midY)
    }
    const last = l.points[l.points.length - 1]!
    g.lineTo(last.x, last.y)
    g.stroke({ color: l.color, width: l.lineWidth, cap: 'round', join: 'round' })
  }

  private drawTextLayer(node: Container, l: EditorTextLayer): void {
    if (!l.text) return
    const hash = textLayerHash(l)
    let cached = this.textTextureCache.get(l.id)
    if (!cached || cached.hash !== hash) {
      const { canvas, cx, cy } = renderTextToCanvas(l)
      const texture = Texture.from(canvas)
      if (cached) cached.texture.destroy(true)
      cached = { texture, hash }
      this.textTextureCache.set(l.id, cached)
      void cx; void cy
    }
    const sprite = new Sprite(cached.texture)
    const bounds = getEditorLayerBounds(l)
    sprite.position.set(bounds.x - PAD, bounds.y - PAD)
    node.addChild(sprite)
    this.setRotation(node, l.x, l.y, l.rotation ?? 0)
  }

  private drawStickerLayer(node: Container, l: StickerLayer): void {
    const hash = stickerLayerHash(l)
    let cached = this.textTextureCache.get(l.id)
    if (!cached || cached.hash !== hash) {
      const { canvas } = renderStickerToCanvas(l)
      const texture = Texture.from(canvas)
      if (cached) cached.texture.destroy(true)
      cached = { texture, hash }
      this.textTextureCache.set(l.id, cached)
    }
    const sprite = new Sprite(cached.texture)
    sprite.position.set(l.x - l.w / 2 - PAD, l.y - l.h / 2 - PAD)
    node.addChild(sprite)
    this.setRotation(node, l.x, l.y, l.rotation ?? 0)
  }

  private drawImageLayer(node: Container, l: EditorImageLayer, layerImages: Map<string, HTMLImageElement>): void {
    const img = layerImages.get(l.id)
    if (!img) return
    const tex = Texture.from(img)
    const sprite = new Sprite(tex)
    sprite.anchor.set(0.5)
    sprite.position.set(l.x, l.y)
    sprite.width = l.w
    sprite.height = l.h
    sprite.alpha = l.opacity
    node.addChild(sprite)
    this.setRotation(node, l.x, l.y, l.rotation ?? 0)
  }

  private drawMosaicLayer(node: Container, l: MosaicLayer): void {
    const src = this.state?.sourceImg
    if (!src) return
    const sourceSize = getEditorRasterSize(src)
    if (!sourceSize.width) return

    const imgOffset = this.state?.imgOffset ?? { x: 0, y: 0 }
    const rx = Math.round(l.x), ry = Math.round(l.y)
    const rw = Math.round(l.w), rh = Math.round(l.h)
    const hash = JSON.stringify({ x: rx, y: ry, w: rw, h: rh, bs: l.blockSize, ox: imgOffset.x, oy: imgOffset.y, sw: sourceSize.width, sh: sourceSize.height })
    const cached = this.mosaicTextureCache.get(l.id)
    if (cached && cached.hash === hash) {
      const sprite = new Sprite(cached.texture)
      sprite.x = rx; sprite.y = ry
      node.addChild(sprite)
      return
    }

    const bs = Math.max(2, l.blockSize)
    const sw = Math.max(1, Math.floor(rw / bs))
    const sh = Math.max(1, Math.floor(rh / bs))

    const small = document.createElement('canvas')
    small.width = sw; small.height = sh
    const sctx = small.getContext('2d')!
    sctx.imageSmoothingEnabled = true
    const sx = rx - imgOffset.x
    const sy = ry - imgOffset.y
    sctx.drawImage(src, sx, sy, rw, rh, 0, 0, sw, sh)

    const canvas = document.createElement('canvas')
    canvas.width = rw; canvas.height = rh
    const ctx = canvas.getContext('2d')!
    ctx.imageSmoothingEnabled = false
    ctx.drawImage(small, 0, 0, sw, sh, 0, 0, rw, rh)

    const texture = Texture.from(canvas)
    texture.source.scaleMode = 'nearest'
    if (cached) cached.texture.destroy(true)
    this.mosaicTextureCache.set(l.id, { texture, hash })

    const sprite = new Sprite(texture)
    sprite.x = rx; sprite.y = ry
    node.addChild(sprite)
  }

  private destroyLayerNode(node: Container, id: string): void {
    node.removeChildren().forEach(c => c.destroy())
    node.destroy()
    const cached = this.textTextureCache.get(id)
    if (cached) {
      cached.texture.destroy(true)
      this.textTextureCache.delete(id)
    }
    const mosaicCached = this.mosaicTextureCache.get(id)
    if (mosaicCached) {
      mosaicCached.texture.destroy(true)
      this.mosaicTextureCache.delete(id)
    }
  }

  destroy(): void {
    this.lifecycleGeneration++
    for (const [id, node] of this.layerNodes) {
      this.destroyLayerNode(node, id)
    }
    this.layerNodes.clear()
    this.layerHashes.clear()
    this.textTextureCache.clear()
    this.mosaicTextureCache.clear()
    this.bgGradientTexture?.destroy(true)
    this.bgGradientTexture = null
    this.bgImageTexture?.destroy(true)
    this.bgImageTexture = null
    this.lastBgGradientKey = null
    this.lastBgImageSource = null
    this.lastBgColor = null
    this.lastCanvasW = 0
    this.lastCanvasH = 0
    this.checkerTexture?.destroy(true)
    this.checkerTexture = null
    this.app?.destroy(true)
    this.app = null
    this.ready = false
  }
}
