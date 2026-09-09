import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { h, ref } from 'vue'
import MessageChatInput from '@/views/messages/MessageChatInput.vue'
import inputSource from '@/views/messages/MessageChatInput.vue?raw'
import viewSource from '@/views/messages/MessagesView.vue?raw'
import chatStylesSource from '@/styles/_chat.scss?raw'
import type { ChatCaptureDestination } from '@/lib/screen-capture/tauri-capture-adapter'
import { sendCapturedMms, snapshotMessageCaptureDestination } from '@/views/messages/message-capture'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

const wrappers: Array<ReturnType<typeof mount>> = []

afterEach(() => {
  while (wrappers.length) wrappers.pop()!.unmount()
})

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })
  return { promise, resolve }
}

function destination(overrides: Partial<ChatCaptureDestination> = {}): ChatCaptureDestination {
  return {
    chatId: '+15551234567',
    channelId: 'thread-1',
    appDir: '/plain',
    ...overrides,
  }
}

describe('SMS composer capture trigger', () => {
  it('keeps capture ownership out of the input and uses a lazy Tauri adapter in MessagesView', () => {
    expect(inputSource).not.toContain('screen-capture/tauri-capture-adapter')
    expect(viewSource).not.toMatch(/import\s+[^\n]+from ['"]@tauri-apps/)
    expect(viewSource).toContain("import('@/lib/screen-capture/tauri-capture-adapter')")
    expect(viewSource).toContain('createChatCaptureTarget')
    expect(viewSource).toContain('captureTarget?.deactivate()')
    expect(viewSource).toContain('captureTarget?.dispose()')
    expect(viewSource).toContain('captureTargetPromise = null')
  })

  it('lays out every Tauri SMS composer action without clipping the capture button', () => {
    expect(inputSource).toContain("'capture-enabled': isTauri")
    expect(chatStylesSource).toMatch(/\.chat-input\.capture-enabled[\s\S]*?\.leading-icons\s*\{[\s\S]*?flex-direction:\s*row/)
  })

  it('renders only in Tauri after attachments and emits a request without capturing itself', async () => {
    const wrapper = mount(MessageChatInput, {
      props: {
        modelValue: '',
        pendingFiles: [],
        totalPendingSize: 0,
        hasLargeNonImageFile: false,
        warnSize: 300 * 1024,
        sendDisabled: false,
        captureDisabled: false,
        sims: [],
        selectedSimId: -1,
      },
      global: {
        mocks: { $t: (key: string) => key },
        directives: { tooltip: () => undefined },
        stubs: {
          EmojiTextField: {
            setup(_props: unknown, { slots }: any) {
              return () => h('div', [slots['leading-icon']?.(), slots['trailing-icon']?.()])
            },
          },
          VIconButton: {
            inheritAttrs: false,
            emits: ['click'],
            setup(_props: unknown, { attrs, emit, slots }: any) {
              return () => h('button', { ...attrs, onClick: () => emit('click') }, slots.default?.())
            },
          },
          SimSelector: true,
        },
      },
    })
    wrappers.push(wrapper)

    const button = wrapper.find('[data-testid="sms-screen-capture-button"]')
    expect(button.exists()).toBe(__IS_TAURI__)
    if (__IS_TAURI__) {
      expect(inputSource.indexOf("$emit('openFilePicker')")).toBeLessThan(inputSource.indexOf("$emit('requestCapture')"))
      await button.trigger('click')
      expect(wrapper.emitted('requestCapture')).toHaveLength(1)
    }
  })
})

describe('captured MMS delivery', () => {
  it('freezes the exact SMS destination and rejects stale targets before touching the draft', async () => {
    const frozen = snapshotMessageCaptureDestination(destination())
    const messageBody = ref('keep this draft')
    const existing = new File(['old'], 'old.jpg', { type: 'image/jpeg' })
    const pendingFiles = ref([existing])
    const sendMessage = vi.fn(async () => true)

    expect(Object.isFrozen(frozen)).toBe(true)
    await expect(
      sendCapturedMms(new File(['png'], 'capture.png', { type: 'image/png' }), frozen, {
        currentDestination: () => destination({ channelId: 'thread-2' }),
        messageBody,
        pendingFiles,
        sendMessage,
      })
    ).rejects.toThrow('capture target changed')

    expect(sendMessage).not.toHaveBeenCalled()
    expect(messageBody.value).toBe('keep this draft')
    expect(pendingFiles.value).toEqual([existing])
  })

  it('awaits MMS completion while preserving an existing draft and attachments', async () => {
    const gate = deferred<boolean>()
    let liveDestination = destination()
    const frozen = snapshotMessageCaptureDestination(liveDestination)
    const messageBody = ref('keep this draft')
    const existing = new File(['old'], 'old.jpg', { type: 'image/jpeg' })
    const capture = new File(['png'], 'capture.png', { type: 'image/png' })
    const pendingFiles = ref([existing])
    let submittedBody = ''
    let submittedFiles: File[] = []
    let submittedDestination: ChatCaptureDestination | undefined
    const sendMessage = vi.fn(() => {
      submittedBody = messageBody.value
      submittedFiles = [...pendingFiles.value]
      submittedDestination = { ...liveDestination }
      return gate.promise
    })
    let settled = false

    const delivery = sendCapturedMms(capture, frozen, {
      currentDestination: () => liveDestination,
      messageBody,
      pendingFiles,
      sendMessage,
    }).finally(() => {
      settled = true
    })

    expect(submittedBody).toBe('')
    expect(submittedFiles).toEqual([capture])
    expect(submittedDestination).toEqual(frozen)
    expect(messageBody.value).toBe('keep this draft')
    expect(pendingFiles.value).toEqual([existing])
    expect(settled).toBe(false)

    liveDestination = destination({ chatId: '+15557654321', channelId: 'thread-2' })
    gate.resolve(true)
    await delivery
    expect(settled).toBe(true)
  })

  it('rejects an unsuccessful MMS send so native capture remains retryable', async () => {
    const messageBody = ref('draft')
    const existing = new File(['old'], 'old.jpg', { type: 'image/jpeg' })
    const pendingFiles = ref([existing])

    await expect(
      sendCapturedMms(new File(['png'], 'capture.png', { type: 'image/png' }), destination(), {
        currentDestination: () => destination(),
        messageBody,
        pendingFiles,
        sendMessage: vi.fn(async () => false),
      })
    ).rejects.toThrow('MMS capture send failed')

    expect(messageBody.value).toBe('draft')
    expect(pendingFiles.value).toEqual([existing])
  })
})
