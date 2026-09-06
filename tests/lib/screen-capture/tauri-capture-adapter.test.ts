import { describe, expect, it, vi } from 'vitest'
import {
  createChatCaptureTarget,
  formatCaptureClientError,
  isRegularCaptureWindowLabel,
  snapshotChatCaptureDestination,
  type ChatCaptureDestination,
} from '@/lib/screen-capture/tauri-capture-adapter'
import { CaptureClientError, type CaptureClient, type CaptureConsumerRegistration } from '@/lib/screen-capture/capture-client'

function harness() {
  const registrations: Array<{
    consumer: (file: File) => Promise<void>
    registration: CaptureConsumerRegistration
    activate: ReturnType<typeof vi.fn>
    deactivate: ReturnType<typeof vi.fn>
    dispose: ReturnType<typeof vi.fn>
  }> = []
  const startComposerCapture = vi.fn(async () => ({ sessionId: 'session-1', overlayGeneration: 1, phase: 'active' }))
  const client = {
    registerConsumer: vi.fn((consumer: (file: File) => Promise<void>) => {
      const token = `token-${registrations.length + 1}`
      const activate = vi.fn(() => token)
      const deactivate = vi.fn()
      const dispose = vi.fn()
      const registration: CaptureConsumerRegistration = {
        targetToken: null,
        activate,
        deactivate,
        dispose,
      }
      registrations.push({ consumer, registration, activate, deactivate, dispose })
      return registration
    }),
    startComposerCapture,
  } as unknown as CaptureClient
  const consume = vi.fn(async (_file: File, _destination: ChatCaptureDestination) => {})
  return { client, consume, registrations, startComposerCapture }
}

describe('chat capture target ownership', () => {
  it('freezes an immutable destination for each activation and rotates registrations', async () => {
    const test = harness()
    const target = createChatCaptureTarget(test.client, test.consume)
    const mutable = { chatId: 'peer:first', channelId: '', appDir: '/app/first' }

    const first = target.activate(mutable)
    mutable.chatId = 'peer:changed'
    mutable.appDir = '/app/changed'
    await test.registrations[0].consumer(new File(['png'], 'capture.png', { type: 'image/png' }))

    expect(first).toBe('token-1')
    expect(test.consume).toHaveBeenLastCalledWith(expect.any(File), {
      chatId: 'peer:first',
      channelId: '',
      appDir: '/app/first',
    })
    expect(Object.isFrozen(test.consume.mock.calls[0][1])).toBe(true)

    target.activate({ chatId: 'channel:second', channelId: 'second', appDir: '/app/second' })
    expect(test.registrations[0].deactivate).toHaveBeenCalledOnce()
    expect(test.registrations[0].dispose).toHaveBeenCalledOnce()
    expect(test.registrations[1].activate).toHaveBeenCalledOnce()
  })

  it('deactivates without disposing until replacement or teardown', () => {
    const test = harness()
    const target = createChatCaptureTarget(test.client, test.consume)

    target.activate({ chatId: 'peer:first', channelId: '', appDir: '/app' })
    target.deactivate()

    expect(test.registrations[0].deactivate).toHaveBeenCalledOnce()
    expect(test.registrations[0].dispose).not.toHaveBeenCalled()

    target.dispose()
    expect(test.registrations[0].dispose).toHaveBeenCalledOnce()
  })

  it('starts only while active and refreshes the target when the destination changes', async () => {
    const test = harness()
    const target = createChatCaptureTarget(test.client, test.consume)

    await expect(target.start({ chatId: 'peer:first', channelId: '', appDir: '/app' })).rejects.toThrow('not active')
    target.activate({ chatId: 'peer:first', channelId: '', appDir: '/app' })
    await target.start({ chatId: 'peer:first', channelId: '', appDir: '/app' })
    await target.start({ chatId: 'peer:second', channelId: '', appDir: '/app' })

    expect(test.startComposerCapture).toHaveBeenCalledTimes(2)
    expect(test.registrations).toHaveLength(2)
  })

  it('rejects blank destination fields before registering a consumer', () => {
    const test = harness()
    const target = createChatCaptureTarget(test.client, test.consume)

    expect(() => target.activate({ chatId: '', channelId: '', appDir: '/app' })).toThrow('chat id')
    expect(() => snapshotChatCaptureDestination({ chatId: 'peer:first', channelId: '', appDir: ' ' })).toThrow('app directory')
    expect(test.client.registerConsumer).not.toHaveBeenCalled()
  })

  it('fails closed when replacing a registration cannot complete', async () => {
    const test = harness()
    const target = createChatCaptureTarget(test.client, test.consume)
    const first = { chatId: 'peer:first', channelId: '', appDir: '/app' }
    target.activate(first)
    vi.mocked(test.client.registerConsumer).mockImplementationOnce(() => {
      throw new Error('registration failed')
    })

    expect(() => target.activate({ chatId: 'peer:second', channelId: '', appDir: '/app' })).toThrow('registration failed')
    await expect(target.start(first)).rejects.toThrow('not active')
    expect(test.startComposerCapture).not.toHaveBeenCalled()
  })
})

describe('capture client diagnostics', () => {
  it('excludes utility webviews from capture target registration', () => {
    expect(isRegularCaptureWindowLabel('main')).toBe(true)
    expect(isRegularCaptureWindowLabel('window-chat')).toBe(true)
    expect(isRegularCaptureWindowLabel('window-')).toBe(false)
    expect(isRegularCaptureWindowLabel('media-preview-warm')).toBe(false)
    expect(isRegularCaptureWindowLabel('screen-capture-overlay-7')).toBe(false)
  })

  it('retains the bounded nested cause that identifies a failed delivery stage', () => {
    const error = new CaptureClientError('consumer_failed', 'the capture consumer rejected the PNG', new Error('upload returned no file hash'))

    expect(formatCaptureClientError(error)).toBe('consumer_failed: the capture consumer rejected the PNG; cause: Error: upload returned no file hash')
    expect(formatCaptureClientError(new CaptureClientError('invalid_result', 'x'.repeat(2_000)))).toHaveLength(1_024)
  })

  it('bounds cyclic error causes instead of dropping the diagnostic callback', () => {
    const cause = new Error('cycle')
    cause.cause = cause

    expect(formatCaptureClientError(new CaptureClientError('consumer_failed', 'consumer rejected PNG', cause))).toContain('[circular error cause]')
  })
})
