import { beforeEach, describe, expect, it, vi } from 'vitest'
import { PixiEditorRenderer } from '@/views/image-editor/pixi/PixiEditorRenderer'

const pixiLifecycle = vi.hoisted(() => ({
  applications: [] as Array<{
    canvas: HTMLCanvasElement | null
    finishInit: () => void
    destroy: ReturnType<typeof vi.fn>
  }>,
}))

vi.mock('pixi.js', () => {
  class Application {
    canvas: HTMLCanvasElement | null = null
    stage = { addChild: vi.fn() }
    renderer = { resize: vi.fn() }
    destroy = vi.fn()
    private resolveInit: () => void = () => {}

    constructor() {
      pixiLifecycle.applications.push({
        canvas: this.canvas,
        finishInit: () => this.resolveInit(),
        destroy: this.destroy,
      })
    }

    async init(options: { canvas: HTMLCanvasElement }) {
      this.canvas = options.canvas
      const record = pixiLifecycle.applications.at(-1)!
      record.canvas = options.canvas
      await new Promise<void>((resolve) => {
        this.resolveInit = resolve
        record.finishInit = resolve
      })
    }
  }

  class Container {
    addChild() {}
  }

  class Graphics {}
  class Sprite {}
  class TilingSprite {}

  return {
    Application,
    Container,
    Graphics,
    Sprite,
    TilingSprite,
    Texture: { EMPTY: {}, from: () => ({ destroy() {} }) },
  }
})

beforeEach(() => {
  pixiLifecycle.applications.length = 0
})

describe('PixiEditorRenderer lifecycle', () => {
  it('disposes safely while asynchronous renderer initialization is pending', async () => {
    const renderer = new PixiEditorRenderer()
    const pendingInit = renderer.init(document.createElement('canvas'))
    await Promise.resolve()

    const application = pixiLifecycle.applications[0]!
    renderer.destroy()
    application.finishInit()

    await expect(pendingInit).resolves.toBeUndefined()
    expect(application.destroy).toHaveBeenCalledTimes(1)
    expect(renderer.isReady).toBe(false)
  })
})
