import { afterEach, describe, expect, it, vi } from 'vitest'
import { subscribeNativeEvents } from '@/lib/api/native-events'
import { chachaEncrypt, chachaDecrypt } from '@/lib/api/crypto'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'

const invoke = vi.hoisted(() => vi.fn())
const channels = vi.hoisted(() => [] as any[])

vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    id = channels.length
    onmessage: (frame: ArrayBuffer) => void = () => {}
    constructor() {
      channels.push(this)
    }
  },
}))

const encodeFrame = (name: string, payload: Uint8Array): ArrayBuffer => {
  const frame = new Uint8Array(name.length + 1 + payload.length)
  frame.set(new TextEncoder().encode(name), 0)
  frame[name.length] = 0
  frame.set(payload, name.length + 1)
  return frame.buffer
}

afterEach(() => {
  channels.length = 0
  invoke.mockReset()
})

describe('subscribeNativeEvents', () => {
  it('subscribes to the core event feed', () => {
    invoke.mockReturnValue(new Promise(() => {}))

    subscribeNativeEvents(
      () => {},
      () => {}
    )

    expect(invoke).toHaveBeenCalledTimes(1)
    expect(invoke.mock.calls[0][0]).toBe('native_events')
    expect(invoke.mock.calls[0][1].onEvent).toBe(channels[0])
  })

  it('delivers the raw core frames without re-encoding them', () => {
    invoke.mockReturnValue(new Promise(() => {}))
    const frames: ArrayBuffer[] = []

    subscribeNativeEvents(
      (frame) => frames.push(frame),
      () => {}
    )

    const binary = encodeFrame('SCREEN_MIRROR_VIDEO', Uint8Array.from([1, 2, 3, 250]))
    channels[0].onmessage(binary)

    expect(frames).toHaveLength(1)
    expect(new Uint8Array(frames[0])).toEqual(new Uint8Array(binary))
    const parsed = parseWebSocketData(frames[0])
    expect(parsed.type).toBe('SCREEN_MIRROR_VIDEO')
    expect(parsed.data).toEqual(Uint8Array.from([1, 2, 3, 250]))
  })

  it('keeps text events decryptable with the local token key', () => {
    invoke.mockReturnValue(new Promise(() => {}))
    const key = Uint8Array.from({ length: 32 }, (_, i) => i)
    const frames: ArrayBuffer[] = []

    subscribeNativeEvents(
      (frame) => frames.push(frame),
      () => {}
    )

    channels[0].onmessage(encodeFrame('CONTENT_CHANGED', chachaEncrypt(key, '{"changed":true}')))

    const parsed = parseWebSocketData(frames[0])
    expect(parsed.type).toBe('CONTENT_CHANGED')
    expect(chachaDecrypt(key, parsed.data)).toBe('{"changed":true}')
  })

  it('reports the stream ending once the command returns', async () => {
    invoke.mockResolvedValue(undefined)
    const onClosed = vi.fn()

    subscribeNativeEvents(() => {}, onClosed)

    await vi.waitFor(() => expect(onClosed).toHaveBeenCalledTimes(1))
  })

  it('reports the stream ending once the command fails', async () => {
    invoke.mockRejectedValue('Rust core stopped')
    const onClosed = vi.fn()

    subscribeNativeEvents(() => {}, onClosed)

    await vi.waitFor(() => expect(onClosed).toHaveBeenCalledTimes(1))
  })

  it('stops delivering frames and closing after the caller closes the stream', async () => {
    invoke.mockReturnValue(new Promise(() => {}))
    const onFrame = vi.fn()
    const onClosed = vi.fn()

    const stream = subscribeNativeEvents(onFrame, onClosed)
    stream.close()
    channels[0].onmessage(encodeFrame('PERMISSIONS_UPDATED', new Uint8Array()).buffer)

    expect(onFrame).not.toHaveBeenCalled()
  })

  it('ignores a late stream end after the caller closed it', async () => {
    let end: () => void = () => {}
    invoke.mockReturnValue(
      new Promise<void>((resolve) => {
        end = resolve
      })
    )
    const onClosed = vi.fn()

    subscribeNativeEvents(() => {}, onClosed).close()
    end()
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(onClosed).not.toHaveBeenCalled()
  })
})
