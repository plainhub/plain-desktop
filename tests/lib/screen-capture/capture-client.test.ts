import { describe, expect, it, vi } from 'vitest'
import {
  CAPTURE_RESULT_AVAILABLE_EVENT,
  CAPTURE_SESSION_ENDED_EVENT,
  CAPTURE_SESSION_STARTED_EVENT,
  CaptureClientError,
  createCaptureClient,
  type CaptureClientDependencies,
  type CaptureEvent,
  type CaptureListen,
  type CaptureResultAvailable,
  type CaptureResultDescriptor,
  type CaptureSessionEnded,
  type CaptureSessionStarted,
} from '@/lib/screen-capture/capture-client'
import { CAPTURE_OVERLAY_SESSION_ENDED_EVENT } from '@/lib/screen-capture/capture-events'

const PNG_BYTES = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 1, 2, 3, 4])

function resultDescriptor(overrides: Partial<CaptureResultDescriptor> = {}): CaptureResultDescriptor {
  return {
    sessionId: 'session-1',
    resultId: 'result-1',
    width: 3,
    height: 2,
    filename: 'Plain-capture.png',
    mimeType: 'image/png',
    byteLen: PNG_BYTES.byteLength,
    ...overrides,
  }
}

function resultAvailable(overrides: Partial<CaptureResultAvailable> = {}): CaptureResultAvailable {
  return {
    targetToken: 'target-1',
    descriptor: resultDescriptor(),
    ...overrides,
  }
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise
    reject = rejectPromise
  })
  return { promise, resolve, reject }
}

function harness(overrides: Partial<CaptureClientDependencies> = {}) {
  const errors: CaptureClientError[] = []
  const handlers = new Map<string, (event: CaptureEvent<unknown>) => void | Promise<void>>()
  const unlisteners: Array<ReturnType<typeof vi.fn>> = []
  const order: string[] = []
  const listen: CaptureListen = vi.fn(async (event, handler) => {
    order.push(`listen:${event}`)
    handlers.set(event, handler)
    const unlisten = vi.fn()
    unlisteners.push(unlisten)
    return unlisten
  })
  const invoke = vi.fn(async (command: string) => {
    order.push(command)
    if (command === 'screen_capture_start') {
      return { sessionId: 'session-1', overlayGeneration: 7, phase: 'awaiting_presentation' }
    }
    if (command === 'screen_capture_take_result') return PNG_BYTES.slice().buffer
    return undefined
  })
  let nextToken = 1
  const effectiveListen = overrides.listen ?? listen
  const effectiveInvoke = overrides.invoke ?? invoke
  const client = createCaptureClient({
    windowLabel: 'main',
    listen: effectiveListen,
    invoke: effectiveInvoke,
    createTargetToken: () => `target-${nextToken++}`,
    onError: (error) => errors.push(error),
    delay: async () => undefined,
    ...overrides,
  })
  return {
    client,
    errors,
    invoke: effectiveInvoke as typeof invoke,
    listen: effectiveListen as typeof listen,
    onResult: () => handlers.get(CAPTURE_RESULT_AVAILABLE_EVENT) as (event: CaptureEvent<CaptureResultAvailable>) => void | Promise<void>,
    onEnded: () => handlers.get(CAPTURE_SESSION_ENDED_EVENT) as (event: CaptureEvent<CaptureSessionEnded>) => void | Promise<void>,
    onStarted: () => handlers.get(CAPTURE_SESSION_STARTED_EVENT) as (event: CaptureEvent<CaptureSessionStarted>) => void | Promise<void>,
    order,
    unlisteners,
  }
}

async function startedHarness(consumer = vi.fn(async (_file: File) => undefined)) {
  const test = harness()
  const registration = test.client.registerConsumer(consumer)
  registration.activate()
  await test.client.startComposerCapture()
  return { ...test, consumer, registration }
}

describe('CaptureClient target ownership', () => {
  it('does not subscribe the target client to overlay terminal payloads', async () => {
    const test = harness()
    const registration = test.client.registerConsumer(async () => undefined)
    registration.activate()
    await test.client.startComposerCapture()

    expect(CAPTURE_SESSION_ENDED_EVENT).not.toBe(CAPTURE_OVERLAY_SESSION_ENDED_EVENT)
    expect(test.order).toContain(`listen:${CAPTURE_SESSION_ENDED_EVENT}`)
    expect(test.order).not.toContain(`listen:${CAPTURE_OVERLAY_SESSION_ENDED_EVENT}`)
  })

  it('publishes only the opaque active token to the trusted native target registry', async () => {
    const test = harness()
    const registration = test.client.registerConsumer(async () => undefined)

    expect(registration.activate()).toBe('target-1')
    await vi.waitFor(() => {
      expect(test.invoke).toHaveBeenCalledWith('screen_capture_register_target', {
        targetToken: 'target-1',
      })
    })
    expect(test.order.indexOf(`listen:${CAPTURE_SESSION_STARTED_EVENT}`)).toBeLessThan(test.order.indexOf('screen_capture_register_target'))
    expect(JSON.stringify(test.invoke.mock.calls)).not.toContain('chatId')
    expect(JSON.stringify(test.invoke.mock.calls)).not.toContain('channelId')

    registration.deactivate()
    registration.deactivate()
    registration.dispose()
    await vi.waitFor(() => {
      expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_unregister_target')).toEqual([['screen_capture_unregister_target', { targetToken: 'target-1' }]])
    })
  })

  it('serializes native registry mutations so stale deactivation cannot erase a newer token', async () => {
    const firstRegistration = deferred<void>()
    const test = harness({
      invoke: vi.fn(async (command: string, args?: Record<string, unknown>) => {
        if (command === 'screen_capture_register_target' && args?.targetToken === 'target-1') return firstRegistration.promise
        return undefined
      }),
    })
    const registration = test.client.registerConsumer(async () => undefined)

    registration.activate()
    registration.deactivate()
    registration.activate()
    firstRegistration.resolve()

    await vi.waitFor(() => {
      expect(test.invoke.mock.calls.filter(([command]) => command.startsWith('screen_capture_'))).toEqual([
        ['screen_capture_register_target', { targetToken: 'target-1' }],
        ['screen_capture_unregister_target', { targetToken: 'target-1' }],
        ['screen_capture_register_target', { targetToken: 'target-2' }],
      ])
    })
    expect(test.client.activeTarget()).toEqual({ windowLabel: 'main', targetToken: 'target-2' })
  })

  it('adopts a trusted global session for the active token and delivers it exactly once', async () => {
    const consumer = vi.fn(async (_file: File) => undefined)
    const test = harness()
    test.client.registerConsumer(consumer).activate()
    await vi.waitFor(() => expect(test.invoke).toHaveBeenCalledWith('screen_capture_register_target', { targetToken: 'target-1' }))

    await test.onStarted()({ payload: { sessionId: 'global-1', targetToken: 'target-1' } })
    await test.onResult()({
      payload: resultAvailable({ descriptor: resultDescriptor({ sessionId: 'global-1' }) }),
    })

    expect(consumer).toHaveBeenCalledOnce()
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_take_result')).toHaveLength(1)
    expect(test.invoke).toHaveBeenCalledWith('screen_capture_ack_result', {
      sessionId: 'global-1',
      resultId: 'result-1',
      targetToken: 'target-1',
    })
    expect(test.client.activeCapture()).toBeNull()
  })

  it('rejects and invalidates a global session announced for a deactivated token', async () => {
    const consumer = vi.fn(async (_file: File) => undefined)
    const test = harness()
    const registration = test.client.registerConsumer(consumer)
    registration.activate()
    await vi.waitFor(() => expect(test.invoke).toHaveBeenCalledWith('screen_capture_register_target', { targetToken: 'target-1' }))
    registration.deactivate()

    await test.onStarted()({ payload: { sessionId: 'global-stale', targetToken: 'target-1' } })
    await vi.waitFor(() => {
      expect(test.invoke).toHaveBeenCalledWith('screen_capture_invalidate_target', {
        sessionId: 'global-stale',
        targetToken: 'target-1',
      })
    })

    expect(test.client.activeCapture()).toBeNull()
    expect(consumer).not.toHaveBeenCalled()
    expect(test.errors.some((error) => error.code === 'target_unavailable')).toBe(true)
  })

  it('invalidates an unadopted global session while a different local session is active', async () => {
    const test = await startedHarness()

    await test.onStarted()({ payload: { sessionId: 'global-unadopted', targetToken: 'target-1' } })
    await vi.waitFor(() => {
      expect(test.invoke).toHaveBeenCalledWith('screen_capture_invalidate_target', {
        sessionId: 'global-unadopted',
        targetToken: 'target-1',
      })
    })

    expect(test.client.activeCapture()).toEqual({ sessionId: 'session-1', targetToken: 'target-1' })
    expect(test.errors.some((error) => error.code === 'capture_busy')).toBe(true)
  })

  it('registers the result listener before start and freezes the active opaque target', async () => {
    const test = harness()
    const registration = test.client.registerConsumer(async () => undefined)
    expect(registration.activate()).toBe('target-1')

    const started = await test.client.startComposerCapture()

    expect(started.sessionId).toBe('session-1')
    const startIndex = test.order.indexOf('screen_capture_start')
    expect(test.order.indexOf('screen_capture_register_target')).toBeLessThan(startIndex)
    expect(test.order.indexOf(`listen:${CAPTURE_RESULT_AVAILABLE_EVENT}`)).toBeLessThan(startIndex)
    expect(test.order.indexOf(`listen:${CAPTURE_SESSION_ENDED_EVENT}`)).toBeLessThan(startIndex)
    expect(test.invoke).toHaveBeenCalledWith('screen_capture_start', {
      targetWindowLabel: 'main',
      targetToken: 'target-1',
    })
    expect(test.client.activeCapture()).toEqual({ sessionId: 'session-1', targetToken: 'target-1' })
  })

  it('allows the webview to have no global send target and refuses a composer capture without guessing one', async () => {
    const test = harness()

    expect(test.client.activeTarget()).toBeNull()
    await expect(test.client.startComposerCapture()).rejects.toMatchObject({ code: 'target_unavailable' })
    expect(test.listen).not.toHaveBeenCalled()
    expect(test.invoke).not.toHaveBeenCalled()
  })

  it('rotates the token on reactivation so an old cached target cannot become eligible again', () => {
    const test = harness()
    const registration = test.client.registerConsumer(async () => undefined)

    expect(registration.activate()).toBe('target-1')
    registration.deactivate()
    expect(registration.activate()).toBe('target-2')
    expect(test.client.activeTarget()).toEqual({ windowLabel: 'main', targetToken: 'target-2' })
  })

  it.each(['deactivate', 'dispose'] as const)('invalidates the frozen native target when its registration calls %s', async (method) => {
    const test = harness()
    const registration = test.client.registerConsumer(async () => undefined)
    registration.activate()
    await test.client.startComposerCapture()

    expect(() => registration[method]()).not.toThrow()
    await vi.waitFor(() => {
      expect(test.invoke).toHaveBeenCalledWith('screen_capture_invalidate_target', {
        sessionId: 'session-1',
        targetToken: 'target-1',
      })
    })
  })

  it('invalidates once when another cached chat activates and never redirects the frozen session', async () => {
    const first = vi.fn(async (_file: File) => undefined)
    const second = vi.fn(async (_file: File) => undefined)
    const test = harness()
    const firstRegistration = test.client.registerConsumer(first)
    firstRegistration.activate()
    await test.client.startComposerCapture()
    const secondRegistration = test.client.registerConsumer(second)

    secondRegistration.activate()
    firstRegistration.deactivate()
    firstRegistration.dispose()
    await vi.waitFor(() => {
      expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_invalidate_target')).toHaveLength(1)
    })
    await test.onResult()({ payload: resultAvailable() })

    expect(test.invoke).toHaveBeenCalledWith('screen_capture_invalidate_target', {
      sessionId: 'session-1',
      targetToken: 'target-1',
    })
    expect(first).not.toHaveBeenCalled()
    expect(second).not.toHaveBeenCalled()
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_take_result')).toBe(false)
  })

  it('does not invalidate for an unrelated registration or when no session exists', async () => {
    const test = harness()
    const owner = test.client.registerConsumer(async () => undefined)
    const unrelated = test.client.registerConsumer(async () => undefined)

    owner.activate()
    owner.deactivate()
    unrelated.deactivate()
    unrelated.dispose()
    await vi.waitFor(() => {
      expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_unregister_target')).toHaveLength(1)
    })
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_invalidate_target')).toBe(false)

    owner.activate()
    await test.client.startComposerCapture()
    unrelated.deactivate()
    unrelated.dispose()
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_invalidate_target')).toBe(false)
  })

  it('reports asynchronous native invalidation failure without throwing from registration lifecycle', async () => {
    const failure = new Error('native target update failed')
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        if (command === 'screen_capture_invalidate_target') throw failure
        return undefined
      }),
    })
    const registration = test.client.registerConsumer(async () => undefined)
    registration.activate()
    await test.client.startComposerCapture()

    expect(() => registration.deactivate()).not.toThrow()
    await vi.waitFor(() => {
      expect(test.errors.at(-1)).toMatchObject({ code: 'target_invalidation_failed', cause: failure })
    })
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_invalidate_target')).toHaveLength(2)
  })

  it('invalidates after native supplies the session ID when deactivation races capture start', async () => {
    const nativeStart = deferred<unknown>()
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return nativeStart.promise
        return undefined
      }),
    })
    const registration = test.client.registerConsumer(async () => undefined)
    registration.activate()

    const starting = test.client.startComposerCapture()
    registration.deactivate()
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_invalidate_target')).toBe(false)
    nativeStart.resolve({ sessionId: 'session-1', overlayGeneration: 7, phase: 'awaiting_presentation' })
    await starting
    await vi.waitFor(() => {
      expect(test.invoke).toHaveBeenCalledWith('screen_capture_invalidate_target', {
        sessionId: 'session-1',
        targetToken: 'target-1',
      })
    })
  })

  it('rejects duplicate capture starts in the same webview', async () => {
    const nativeStart = deferred<unknown>()
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return nativeStart.promise
        return undefined
      }),
    })
    test.client.registerConsumer(async () => undefined).activate()

    const first = test.client.startComposerCapture()
    await expect(test.client.startComposerCapture()).rejects.toMatchObject({ code: 'capture_busy' })
    nativeStart.resolve({ sessionId: 'session-1', overlayGeneration: 7, phase: 'waiting_for_overlay' })
    await first
  })

  it('clears its frozen start and queued results when native start fails', async () => {
    const consumer = vi.fn(async (_file: File) => undefined)
    let emit!: (event: CaptureEvent<CaptureResultAvailable>) => void | Promise<void>
    let starts = 0
    const invoke = vi.fn(async (command: string) => {
      if (command !== 'screen_capture_start') return undefined
      starts += 1
      if (starts === 1) {
        await emit({ payload: resultAvailable() })
        throw new Error('native start failed')
      }
      return { sessionId: 'session-2', overlayGeneration: 7, phase: 'waiting_for_overlay' }
    })
    const test = harness({
      listen: async (event, handler) => {
        if (event === CAPTURE_RESULT_AVAILABLE_EVENT) {
          emit = handler as (event: CaptureEvent<CaptureResultAvailable>) => void | Promise<void>
        }
        return () => undefined
      },
      invoke,
    })
    test.client.registerConsumer(consumer).activate()

    await expect(test.client.startComposerCapture()).rejects.toMatchObject({ code: 'invalid_start' })
    expect(test.client.activeCapture()).toBeNull()
    await test.client.startComposerCapture()
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(consumer).not.toHaveBeenCalled()
    expect(test.client.activeCapture()).toEqual({ sessionId: 'session-2', targetToken: 'target-1' })
  })

  it('clears its frozen start and can retry when listener installation fails', async () => {
    let listenAttempts = 0
    const partialUnlisten = vi.fn()
    const test = harness({
      listen: async () => {
        listenAttempts += 1
        if (listenAttempts === 1) return partialUnlisten
        if (listenAttempts === 2) throw new Error('event service unavailable')
        return () => undefined
      },
    })
    test.client.registerConsumer(async () => undefined).activate()

    await expect(test.client.startComposerCapture()).rejects.toMatchObject({ code: 'invalid_start' })
    expect(test.client.activeCapture()).toBeNull()
    await expect(test.client.startComposerCapture()).resolves.toMatchObject({ sessionId: 'session-1' })

    expect(listenAttempts).toBe(5)
    expect(partialUnlisten).toHaveBeenCalledOnce()
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_start')).toHaveLength(1)
  })

  it.each(['cancelled', 'saved', 'copied', 'failed', 'completed'] as const)('clears local busy state after a matching %s terminal event', async (outcome) => {
    const test = await startedHarness()

    await test.onEnded()({ payload: { sessionId: 'session-1', targetToken: 'target-1', outcome } })

    expect(test.client.activeCapture()).toBeNull()
    await expect(test.client.startComposerCapture()).resolves.toMatchObject({ sessionId: 'session-1' })
  })

  it('does not invalidate after terminal cleanup already won the deactivation race', async () => {
    const test = await startedHarness()

    await test.onEnded()({ payload: { sessionId: 'session-1', targetToken: 'target-1', outcome: 'cancelled' } })
    test.registration.deactivate()
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_invalidate_target')).toBe(false)
    expect(test.client.activeCapture()).toBeNull()
  })

  it('clears local busy state after native confirms target invalidation', async () => {
    const test = await startedHarness()

    test.registration.deactivate()
    await vi.waitFor(() => expect(test.client.activeCapture()).toBeNull())

    test.registration.activate()
    await expect(test.client.startComposerCapture()).resolves.toMatchObject({ sessionId: 'session-1' })
  })

  it('retries idempotent target invalidation before leaving the session busy', async () => {
    let invalidationAttempts = 0
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        if (command === 'screen_capture_invalidate_target' && invalidationAttempts++ === 0) throw new Error('response interrupted')
        return undefined
      }),
    })
    const registration = test.client.registerConsumer(async () => undefined)
    registration.activate()
    await test.client.startComposerCapture()

    registration.deactivate()
    await vi.waitFor(() => expect(test.client.activeCapture()).toBeNull())

    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_invalidate_target')).toHaveLength(2)
    expect(test.errors).toEqual([])
  })

  it('does not resurrect a session when terminal cleanup races an in-flight invalidation', async () => {
    const invalidation = deferred<unknown>()
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        if (command === 'screen_capture_invalidate_target') return invalidation.promise
        return undefined
      }),
    })
    const registration = test.client.registerConsumer(async () => undefined)
    registration.activate()
    await test.client.startComposerCapture()

    registration.deactivate()
    await test.onEnded()({ payload: { sessionId: 'session-1', targetToken: 'target-1', outcome: 'failed' } })
    invalidation.resolve(undefined)
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_invalidate_target')).toHaveLength(1)
    expect(test.client.activeCapture()).toBeNull()
  })

  it.each([
    ['stale session', { sessionId: 'session-old', targetToken: 'target-1', outcome: 'cancelled' }],
    ['wrong target', { sessionId: 'session-1', targetToken: 'target-stale', outcome: 'failed' }],
    ['invalid outcome', { sessionId: 'session-1', targetToken: 'target-1', outcome: 'sent' }],
  ])('does not clear the frozen session for a %s terminal event', async (_case, payload) => {
    const test = await startedHarness()

    await test.onEnded()({ payload: payload as CaptureSessionEnded })

    expect(test.client.activeCapture()).toEqual({ sessionId: 'session-1', targetToken: 'target-1' })
    expect(test.errors.at(-1)?.code).toMatch(/invalid_result|target_unavailable/)
  })

  it('does not create stuck local state when terminal cleanup races the start response', async () => {
    let emitEnded!: (event: CaptureEvent<CaptureSessionEnded>) => void | Promise<void>
    const test = harness({
      listen: async (event, handler) => {
        if (event === CAPTURE_SESSION_ENDED_EVENT) {
          emitEnded = handler as (event: CaptureEvent<CaptureSessionEnded>) => void | Promise<void>
        }
        return () => undefined
      },
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') {
          await emitEnded({ payload: { sessionId: 'session-1', targetToken: 'target-1', outcome: 'failed' } })
          return { sessionId: 'session-1', overlayGeneration: 7, phase: 'restoring' }
        }
        return undefined
      }),
    })
    test.client.registerConsumer(async () => undefined).activate()

    await test.client.startComposerCapture()

    expect(test.client.activeCapture()).toBeNull()
  })
})

describe('CaptureClient result delivery', () => {
  it('passes exactly one File with the native name, PNG MIME, and exact binary bytes, then acknowledges', async () => {
    const test = await startedHarness()

    await test.onResult()({ payload: resultAvailable() })

    expect(test.consumer).toHaveBeenCalledOnce()
    const file = test.consumer.mock.calls[0][0]
    expect(file).toBeInstanceOf(File)
    expect(file.name).toBe('Plain-capture.png')
    expect(file.type).toBe('image/png')
    expect(Array.from(new Uint8Array(await file.arrayBuffer()))).toEqual(Array.from(PNG_BYTES))
    expect(test.invoke).toHaveBeenCalledWith('screen_capture_take_result', {
      sessionId: 'session-1',
      resultId: 'result-1',
      targetToken: 'target-1',
    })
    expect(test.invoke).toHaveBeenLastCalledWith('screen_capture_ack_result', {
      sessionId: 'session-1',
      resultId: 'result-1',
      targetToken: 'target-1',
    })
    expect(test.client.activeCapture()).toBeNull()
  })

  it('releases the native lease after consumer failure so the same retained result can be retried', async () => {
    const failure = new Error('upload failed')
    const consumer = vi.fn<(_: File) => Promise<void>>().mockRejectedValueOnce(failure).mockResolvedValueOnce(undefined)
    const test = await startedHarness(consumer)

    await test.onResult()({ payload: resultAvailable() })

    expect(test.invoke).toHaveBeenLastCalledWith('screen_capture_release_result', {
      sessionId: 'session-1',
      resultId: 'result-1',
      targetToken: 'target-1',
    })
    expect(test.errors.at(-1)).toMatchObject({ code: 'consumer_failed', cause: failure })
    expect(test.client.activeCapture()).toEqual({ sessionId: 'session-1', targetToken: 'target-1' })

    await test.onResult()({ payload: resultAvailable() })

    expect(consumer).toHaveBeenCalledTimes(2)
    expect(test.invoke).toHaveBeenLastCalledWith('screen_capture_ack_result', expect.any(Object))
  })

  it('lets a committed immutable delivery finish before deactivating its target', async () => {
    const consumed = deferred<void>()
    const consumer = vi.fn(async (_file: File) => consumed.promise)
    const test = await startedHarness(consumer)

    const delivery = test.onResult()({ payload: resultAvailable() })
    await vi.waitFor(() => expect(consumer).toHaveBeenCalledOnce())
    test.registration.deactivate()
    await Promise.resolve()

    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_invalidate_target')).toBe(false)

    consumed.resolve()
    await delivery

    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_ack_result')).toHaveLength(1)
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_invalidate_target')).toBe(false)
    expect(test.client.activeCapture()).toBeNull()
  })

  it('releases a failed committed delivery before honoring deferred target invalidation', async () => {
    const consumed = deferred<void>()
    const consumer = vi.fn(async (_file: File) => consumed.promise)
    const test = await startedHarness(consumer)

    const delivery = test.onResult()({ payload: resultAvailable() })
    await vi.waitFor(() => expect(consumer).toHaveBeenCalledOnce())
    test.registration.deactivate()
    consumed.reject(new Error('upload failed'))
    await delivery
    await vi.waitFor(() => {
      expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_invalidate_target')).toBe(true)
    })

    const commands = test.invoke.mock.calls.map(([command]) => command)
    expect(commands.indexOf('screen_capture_release_result')).toBeLessThan(commands.indexOf('screen_capture_invalidate_target'))
  })

  it('retries only the acknowledgment without requiring another result event', async () => {
    let ackAttempts = 0
    const consumer = vi.fn(async (_file: File) => undefined)
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        if (command === 'screen_capture_take_result') return PNG_BYTES.slice().buffer
        if (command === 'screen_capture_ack_result' && ackAttempts++ === 0) throw new Error('response lost')
        return undefined
      }),
    })
    test.client.registerConsumer(consumer).activate()
    await test.client.startComposerCapture()

    await test.onResult()({ payload: resultAvailable() })

    expect(consumer).toHaveBeenCalledOnce()
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_take_result')).toHaveLength(1)
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_ack_result')).toHaveLength(2)
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_release_result')).toBe(false)
  })

  it('uses bounded backoff across multiple transient acknowledgment failures without redelivery', async () => {
    let ackAttempts = 0
    const delays: number[] = []
    const consumer = vi.fn(async (_file: File) => undefined)
    const test = harness({
      delay: vi.fn(async (milliseconds: number) => {
        delays.push(milliseconds)
      }),
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_register_target') return undefined
        if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        if (command === 'screen_capture_take_result') return PNG_BYTES.slice().buffer
        if (command === 'screen_capture_ack_result' && ackAttempts++ < 2) throw new Error('transient acknowledgment failure')
        return undefined
      }),
    })
    test.client.registerConsumer(consumer).activate()
    await test.client.startComposerCapture()

    await test.onResult()({ payload: resultAvailable() })

    expect(delays).toEqual([50, 200])
    expect(consumer).toHaveBeenCalledOnce()
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_take_result')).toHaveLength(1)
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_ack_result')).toHaveLength(3)
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_release_result')).toBe(false)
  })

  it('does not resurrect state when a terminal event wins a late acknowledgment-error race', async () => {
    const consumer = vi.fn(async (_file: File) => undefined)
    let test!: ReturnType<typeof harness>
    test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        if (command === 'screen_capture_take_result') return PNG_BYTES.slice().buffer
        if (command === 'screen_capture_ack_result') {
          await test.onEnded()({
            payload: { sessionId: 'session-1', targetToken: 'target-1', outcome: 'completed' },
          })
          throw new Error('late IPC rejection')
        }
        return undefined
      }),
    })
    test.client.registerConsumer(consumer).activate()
    await test.client.startComposerCapture()

    await test.onResult()({ payload: resultAvailable() })

    expect(consumer).toHaveBeenCalledOnce()
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_ack_result')).toHaveLength(1)
    expect(test.client.activeCapture()).toBeNull()
    expect(test.errors.some((error) => error.code === 'acknowledgment_failed')).toBe(false)
  })

  it('stops acknowledgment backoff when terminal cleanup wins during the delay', async () => {
    const consumer = vi.fn(async (_file: File) => undefined)
    let test!: ReturnType<typeof harness>
    test = harness({
      delay: vi.fn(async () => {
        await test.onEnded()({
          payload: { sessionId: 'session-1', targetToken: 'target-1', outcome: 'completed' },
        })
      }),
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        if (command === 'screen_capture_take_result') return PNG_BYTES.slice().buffer
        if (command === 'screen_capture_ack_result') throw new Error('transient acknowledgment failure')
        return undefined
      }),
    })
    test.client.registerConsumer(consumer).activate()
    await test.client.startComposerCapture()

    await test.onResult()({ payload: resultAvailable() })

    expect(consumer).toHaveBeenCalledOnce()
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_ack_result')).toHaveLength(1)
    expect(test.client.activeCapture()).toBeNull()
    expect(test.errors.some((error) => error.code === 'acknowledgment_failed')).toBe(false)
  })

  it('does not redirect a frozen session when another cached consumer activates', async () => {
    const first = vi.fn(async (_file: File) => undefined)
    const second = vi.fn(async (_file: File) => undefined)
    const test = harness()
    test.client.registerConsumer(first).activate()
    await test.client.startComposerCapture()
    test.client.registerConsumer(second).activate()

    await test.onResult()({ payload: resultAvailable() })

    expect(first).not.toHaveBeenCalled()
    expect(second).not.toHaveBeenCalled()
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_take_result')).toBe(false)
    expect(test.errors.at(-1)).toMatchObject({ code: 'target_unavailable' })
  })

  it('does not deliver or release after native target invalidation wins an in-flight raw read', async () => {
    const bytes = deferred<unknown>()
    const consumer = vi.fn(async (_file: File) => undefined)
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        if (command === 'screen_capture_take_result') return bytes.promise
        return undefined
      }),
    })
    const registration = test.client.registerConsumer(consumer)
    registration.activate()
    await test.client.startComposerCapture()

    const delivery = test.onResult()({ payload: resultAvailable() })
    registration.deactivate()
    bytes.resolve(PNG_BYTES.slice().buffer)
    await delivery

    expect(consumer).not.toHaveBeenCalled()
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_release_result')).toBe(false)
    expect(test.errors).toEqual([])
  })

  it('serializes duplicate result events so the consumer and native lease are acquired once', async () => {
    const consumed = deferred<void>()
    const consumer = vi.fn(async (_file: File) => consumed.promise)
    const test = await startedHarness(consumer)

    const first = test.onResult()({ payload: resultAvailable() })
    await vi.waitFor(() => expect(consumer).toHaveBeenCalledOnce())
    await test.onResult()({ payload: resultAvailable() })
    consumed.resolve()
    await first

    expect(consumer).toHaveBeenCalledOnce()
    expect(test.invoke.mock.calls.filter(([command]) => command === 'screen_capture_take_result')).toHaveLength(1)
    expect(test.errors.at(-1)).toMatchObject({ code: 'delivery_busy' })
  })

  it.each([
    ['wrong session', resultAvailable({ descriptor: resultDescriptor({ sessionId: 'session-old' }) })],
    ['wrong target token', resultAvailable({ targetToken: 'target-stale' })],
    ['wrong MIME', resultAvailable({ descriptor: resultDescriptor({ mimeType: 'image/jpeg' }) })],
    ['unsafe filename', resultAvailable({ descriptor: resultDescriptor({ filename: '../capture.png' }) })],
    ['invalid size', resultAvailable({ descriptor: resultDescriptor({ byteLen: 0 }) })],
    ['oversized PNG', resultAvailable({ descriptor: resultDescriptor({ byteLen: 160 * 1024 * 1024 + 1 }) })],
  ])('fails closed for %s metadata before reading result bytes', async (_case, payload) => {
    const test = await startedHarness()

    await test.onResult()({ payload })

    expect(test.consumer).not.toHaveBeenCalled()
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_take_result')).toBe(false)
    expect(test.errors.at(-1)?.code).toMatch(/invalid_result|target_unavailable/)
  })

  it('validates raw IPC type, length, and PNG signature and releases an acquired lease on mismatch', async () => {
    for (const invalid of [new Uint8Array(PNG_BYTES).buffer.slice(0, -1), new Uint8Array(PNG_BYTES.byteLength).buffer, [...PNG_BYTES]]) {
      const consumer = vi.fn(async (_file: File) => undefined)
      const test = harness({
        invoke: vi.fn(async (command: string) => {
          if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
          if (command === 'screen_capture_take_result') return invalid
          return undefined
        }),
      })
      test.client.registerConsumer(consumer).activate()
      await test.client.startComposerCapture()

      await test.onResult()({ payload: resultAvailable() })

      expect(consumer).not.toHaveBeenCalled()
      expect(test.invoke).toHaveBeenLastCalledWith('screen_capture_release_result', expect.any(Object))
      expect(test.errors.at(-1)).toMatchObject({ code: 'invalid_result' })
    }
  })

  it('aborts delivery when taking the result rejects before lease ownership is known', async () => {
    const consumer = vi.fn(async (_file: File) => undefined)
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') {
          return { sessionId: 'session-1', overlayGeneration: 7, phase: 'result_available' }
        }
        if (command === 'screen_capture_take_result') throw new Error('ambiguous raw IPC failure')
        return undefined
      }),
    })
    test.client.registerConsumer(consumer).activate()
    await test.client.startComposerCapture()

    await test.onResult()({ payload: resultAvailable() })

    expect(consumer).not.toHaveBeenCalled()
    expect(test.invoke).toHaveBeenCalledWith('screen_capture_release_result', {
      sessionId: 'session-1',
      resultId: 'result-1',
      targetToken: 'target-1',
    })
  })

  it('does not consume or release a late raw response after terminal cleanup', async () => {
    const bytes = deferred<unknown>()
    const consumer = vi.fn(async (_file: File) => undefined)
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        if (command === 'screen_capture_take_result') return bytes.promise
        return undefined
      }),
    })
    test.client.registerConsumer(consumer).activate()
    await test.client.startComposerCapture()

    const delivery = test.onResult()({ payload: resultAvailable() })
    await test.onEnded()({ payload: { sessionId: 'session-1', targetToken: 'target-1', outcome: 'cancelled' } })
    bytes.resolve(PNG_BYTES.slice().buffer)
    await delivery

    expect(consumer).not.toHaveBeenCalled()
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_release_result')).toBe(false)
    expect(test.client.activeCapture()).toBeNull()
    expect(test.errors.some((error) => error.code === 'lease_release_failed')).toBe(false)
  })

  it('queues a result emitted after listener registration but before start returns', async () => {
    const consumer = vi.fn(async (_file: File) => undefined)
    let emit!: (event: CaptureEvent<CaptureResultAvailable>) => void | Promise<void>
    const invoke = vi.fn(async (command: string) => {
      if (command === 'screen_capture_start') {
        await emit({ payload: resultAvailable() })
        return { sessionId: 'session-1', overlayGeneration: 7, phase: 'result_available' }
      }
      if (command === 'screen_capture_take_result') return PNG_BYTES.slice().buffer
      return undefined
    })
    const test = harness({
      listen: async (event, handler) => {
        if (event === CAPTURE_RESULT_AVAILABLE_EVENT) {
          emit = handler as (event: CaptureEvent<CaptureResultAvailable>) => void | Promise<void>
        }
        return () => undefined
      },
      invoke,
    })
    test.client.registerConsumer(consumer).activate()

    await test.client.startComposerCapture()
    await vi.waitFor(() => expect(consumer).toHaveBeenCalledOnce())

    expect(invoke.mock.calls.map(([command]) => command)).toEqual(['screen_capture_register_target', 'screen_capture_start', 'screen_capture_take_result', 'screen_capture_ack_result'])
  })

  it('unlistens and invalidates the target on disposal', async () => {
    const test = harness()
    test.client.registerConsumer(async () => undefined).activate()
    await test.client.startComposerCapture()

    test.client.dispose()
    await test.onResult()({ payload: resultAvailable() })

    expect(test.unlisteners).toHaveLength(3)
    for (const unlisten of test.unlisteners) expect(unlisten).toHaveBeenCalledOnce()
    expect(test.client.activeTarget()).toBeNull()
    expect(test.invoke.mock.calls.some(([command]) => command === 'screen_capture_take_result')).toBe(false)
    expect(test.invoke).toHaveBeenCalledWith('screen_capture_invalidate_target', {
      sessionId: 'session-1',
      targetToken: 'target-1',
    })
  })

  it('retries target invalidation after disposal clears the local session', async () => {
    let invalidationAttempts = 0
    const test = harness({
      invoke: vi.fn(async (command: string) => {
        if (command === 'screen_capture_start') {
          return { sessionId: 'session-1', overlayGeneration: 7, phase: 'active' }
        }
        if (command === 'screen_capture_invalidate_target') {
          invalidationAttempts += 1
          if (invalidationAttempts === 1) throw new Error('ambiguous IPC failure')
        }
        return undefined
      }),
    })
    test.client.registerConsumer(async () => undefined).activate()
    await test.client.startComposerCapture()

    test.client.dispose()

    await vi.waitFor(() => expect(invalidationAttempts).toBe(2))
    expect(test.errors.some((error) => error.code === 'target_invalidation_failed')).toBe(false)
  })
})
