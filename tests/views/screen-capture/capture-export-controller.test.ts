import { describe, expect, it, vi } from 'vitest'
import { createCaptureExportController, type CaptureExportControllerInvoke } from '@/views/screen-capture/capture-export-controller'

const PNG_BYTES = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 1, 2, 3, 4])

function harness() {
  let resultNumber = 0
  const invoke = vi.fn<CaptureExportControllerInvoke>(async (command, args) => {
    if (command === 'screen_capture_submit_result') {
      resultNumber += 1
      const byteLen = args instanceof ArrayBuffer ? args.byteLength : 0
      return {
        sessionId: 'session-1',
        resultId: `result-${resultNumber}`,
        width: 40,
        height: 30,
        filename: `Plain-capture-${resultNumber}.png`,
        mimeType: 'image/png',
        byteLen,
      }
    }
    if (command === 'screen_capture_save_result') return 'saved'
    return undefined
  })
  const controller = createCaptureExportController({
    sessionId: 'session-1',
    overlayGeneration: 7,
    invoke,
  })
  const payload = {
    png: new Blob([PNG_BYTES], { type: 'image/png' }),
    selection: { x: 10, y: 15, width: 40, height: 30 },
  }
  return { controller, invoke, payload }
}

describe('CaptureExportController', () => {
  it('submits PNG bytes through raw IPC before publishing only metadata to the target', async () => {
    const test = harness()

    await test.controller.export('confirm', test.payload)

    expect(test.invoke.mock.calls.map(([command]) => command)).toEqual(['screen_capture_submit_result', 'screen_capture_send_result'])
    const [, body, options] = test.invoke.mock.calls[0]!
    expect(body).toBeInstanceOf(ArrayBuffer)
    expect(Array.from(new Uint8Array(body as ArrayBuffer))).toEqual(Array.from(PNG_BYTES))
    expect(options).toEqual({
      headers: {
        'x-plain-capture-session-id': 'session-1',
        'x-plain-capture-overlay-generation': '7',
        'x-plain-capture-width': '40',
        'x-plain-capture-height': '30',
      },
    })
    expect(test.invoke).toHaveBeenLastCalledWith('screen_capture_send_result', {
      sessionId: 'session-1',
      resultId: 'result-1',
      overlayGeneration: 7,
    })
  })

  it.each([
    ['save', 'screen_capture_save_result'],
    ['copy', 'screen_capture_copy_result'],
  ] as const)('routes %s through its validated native export command', async (action, command) => {
    const test = harness()

    await test.controller.export(action, test.payload)

    expect(test.invoke).toHaveBeenLastCalledWith(command, {
      sessionId: 'session-1',
      resultId: 'result-1',
      overlayGeneration: 7,
    })
  })

  it('keeps a cancelled save retryable and discards its stale result before re-rendered bytes', async () => {
    const test = harness()
    test.invoke.mockImplementationOnce(async () => ({
      sessionId: 'session-1',
      resultId: 'result-old',
      width: 40,
      height: 30,
      filename: 'Plain-capture-old.png',
      mimeType: 'image/png',
      byteLen: PNG_BYTES.byteLength,
    }))
    test.invoke.mockImplementationOnce(async () => 'cancelled')

    await test.controller.export('save', test.payload)
    await test.controller.export('copy', test.payload)

    expect(test.invoke.mock.calls.map(([command]) => command)).toEqual([
      'screen_capture_submit_result',
      'screen_capture_save_result',
      'screen_capture_discard_result',
      'screen_capture_submit_result',
      'screen_capture_copy_result',
    ])
    expect(test.invoke).toHaveBeenNthCalledWith(3, 'screen_capture_discard_result', {
      sessionId: 'session-1',
      resultId: 'result-old',
      overlayGeneration: 7,
    })
  })

  it('retains a submitted result after native failure so a retry first resets native state', async () => {
    const test = harness()
    test.invoke.mockImplementationOnce(async () => ({
      sessionId: 'session-1',
      resultId: 'result-old',
      width: 40,
      height: 30,
      filename: 'Plain-capture-old.png',
      mimeType: 'image/png',
      byteLen: PNG_BYTES.byteLength,
    }))
    test.invoke.mockImplementationOnce(async () => {
      throw new Error('clipboard denied')
    })

    await expect(test.controller.export('copy', test.payload)).rejects.toThrow('clipboard denied')
    await test.controller.export('copy', test.payload)

    expect(test.invoke.mock.calls.map(([command]) => command)).toEqual([
      'screen_capture_submit_result',
      'screen_capture_copy_result',
      'screen_capture_discard_result',
      'screen_capture_submit_result',
      'screen_capture_copy_result',
    ])
  })

  it('fails closed on malformed native descriptors before any terminal action', async () => {
    const test = harness()
    test.invoke.mockResolvedValueOnce({
      sessionId: 'another-session',
      resultId: 'result-1',
      width: 40,
      height: 30,
      filename: 'capture.png',
      mimeType: 'image/png',
      byteLen: PNG_BYTES.byteLength,
    })

    await expect(test.controller.export('confirm', test.payload)).rejects.toThrow(/descriptor/i)
    expect(test.invoke.mock.calls.map(([command]) => command)).toEqual(['screen_capture_submit_result', 'screen_capture_fail'])
    expect(test.invoke).toHaveBeenLastCalledWith('screen_capture_fail', {
      sessionId: 'session-1',
      overlayGeneration: 7,
      code: 'protocol_mismatch',
      detail: 'native capture result descriptor is invalid',
    })
  })

  it('rejects invalid PNG payloads before crossing IPC', async () => {
    const test = harness()

    await expect(
      test.controller.export('copy', {
        ...test.payload,
        png: new Blob([new Uint8Array([1, 2, 3])], { type: 'image/png' }),
      })
    ).rejects.toThrow(/PNG/i)
    await expect(
      test.controller.export('copy', {
        ...test.payload,
        png: new Blob([PNG_BYTES], { type: 'image/jpeg' }),
      })
    ).rejects.toThrow(/PNG/i)

    expect(test.invoke).not.toHaveBeenCalled()
  })

  it('serializes actions and refuses use after terminal disposal', async () => {
    let release!: () => void
    const pending = new Promise<void>((resolve) => {
      release = resolve
    })
    const test = harness()
    test.invoke.mockImplementationOnce(async () => {
      await pending
      return {
        sessionId: 'session-1',
        resultId: 'result-1',
        width: 40,
        height: 30,
        filename: 'capture.png',
        mimeType: 'image/png',
        byteLen: PNG_BYTES.byteLength,
      }
    })

    const first = test.controller.export('copy', test.payload)
    await expect(test.controller.export('save', test.payload)).rejects.toThrow(/progress/i)
    release()
    await first
    test.controller.dispose()

    await expect(test.controller.export('copy', test.payload)).rejects.toThrow(/disposed/i)
  })
})
