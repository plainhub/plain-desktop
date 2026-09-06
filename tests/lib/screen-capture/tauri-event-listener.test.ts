import { describe, expect, it, vi } from 'vitest'
import { listenToCaptureWindow } from '@/lib/screen-capture/tauri-event-listener'

describe('listenToCaptureWindow', () => {
  it('scopes native targeted events to the exact webview-window label', async () => {
    const unlisten = vi.fn()
    const listen = vi.fn(async () => unlisten)
    const handler = vi.fn()

    await listenToCaptureWindow(listen, 'screen-capture-overlay-7', 'screen-capture://frame-available', handler)

    expect(listen).toHaveBeenCalledWith('screen-capture://frame-available', handler, {
      target: 'screen-capture-overlay-7',
    })
  })

  it('rejects an empty event target instead of silently listening to Any', async () => {
    const listen = vi.fn(async () => vi.fn())

    await expect(listenToCaptureWindow(listen, ' ', 'screen-capture://frame-available', vi.fn())).rejects.toThrow(/window label/i)
    expect(listen).not.toHaveBeenCalled()
  })
})
