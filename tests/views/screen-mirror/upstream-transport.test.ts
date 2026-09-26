import { afterEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/hooks/app-socket', () => ({
  sendAppWsBytes: vi.fn(() => true),
  sendAppWsJson: vi.fn(() => true),
}))

import { sendAppWsJson } from '@/hooks/app-socket'
import { defineComponent, h, ref } from 'vue'
import { mount } from '@vue/test-utils'
import { useScreenMirrorControl, type ScreenMirrorControlEvent } from '@/views/screen-mirror/screen-mirror-control'

// Locks the upstream transport contract (API_SPEC §12.3/§12.6): the WS is
// the only control channel — the composable's sendControl must ride the
// typed JSON envelope, and when the socket is down the frame is dropped
// (the GraphQL mutation was removed; no fallback transport may return).
// Touch samples bypass this path entirely (binary frames, locked in
// touch-frame tests).
const Probe = defineComponent({
  setup(_, { expose }) {
    const canvasRef = ref<HTMLCanvasElement>()
    const enabled = ref(true)
    const control = useScreenMirrorControl(canvasRef, enabled)
    expose({ control })
    return () => h('div')
  },
})

function mountProbe() {
  return mount(Probe).vm as unknown as {
    control: ReturnType<typeof useScreenMirrorControl>
  }
}

describe('useScreenMirrorControl upstream transport', () => {
  afterEach(() => {
    vi.clearAllMocks()
  })

  it('wraps control events in the screenMirrorControl envelope on the WS', () => {
    const vm = mountProbe()
    const event: ScreenMirrorControlEvent = { action: 'BACK' }
    vm.control.sendControl(event)
    expect(sendAppWsJson).toHaveBeenCalledWith({ type: 'screenMirrorControl', input: event })
  })

  it('drops the frame silently when the socket is down — no fallback transport', async () => {
    vi.mocked(sendAppWsJson).mockReturnValue(false)
    const vm = mountProbe()
    const event: ScreenMirrorControlEvent = { action: 'SCROLL', x: 0.5, y: 0.5, deltaX: 0, deltaY: 120 }
    expect(() => vm.control.sendControl(event)).not.toThrow()
    expect(sendAppWsJson).toHaveBeenCalledWith({ type: 'screenMirrorControl', input: event })
    await Promise.resolve()
    await Promise.resolve()
  })
})
