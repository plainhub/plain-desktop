import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { h } from 'vue'
import ChatInput from '@/views/chat/ChatInput.vue'
import chatInputSource from '@/views/chat/ChatInput.vue?raw'
import chatViewSource from '@/views/chat/ChatView.vue?raw'
import tauriAdapterSource from '@/lib/screen-capture/tauri-capture-adapter.ts?raw'
import chatStylesSource from '@/styles/_chat.scss?raw'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

const wrappers: Array<ReturnType<typeof mount>> = []

afterEach(() => {
  while (wrappers.length) wrappers.pop()!.unmount()
})

describe('ChatInput screen capture action', () => {
  it('keeps native ownership out of the input and orders the action after image and folder', () => {
    expect(chatInputSource).not.toContain('screen-capture/tauri-capture-adapter')
    expect(chatInputSource.indexOf('@click="sendImages"')).toBeLessThan(chatInputSource.indexOf('@click="sendFiles"'))
    expect(chatInputSource.indexOf('@click="sendFiles"')).toBeLessThan(chatInputSource.indexOf('@click="$emit(\'request-capture\')"'))
  })

  it('lays out all three Tauri composer actions without clipping the capture button', () => {
    expect(chatInputSource).toContain("'capture-enabled': isTauri")
    expect(chatStylesSource).toMatch(/\.chat-input\.capture-enabled[\s\S]*?\.leading-icons\s*\{[\s\S]*?flex-direction:\s*row/)
    expect(chatStylesSource).toMatch(/\.chat-input\.capture-enabled[\s\S]*?\.field-input\s*\{[\s\S]*?padding-left:\s*128px/)
  })

  it('keeps the web graph free of eager Tauri loading and wires view lifecycle ownership', () => {
    expect(chatViewSource).not.toMatch(/import\s+[^\n]+from ['"]@tauri-apps/)
    expect(chatViewSource).toContain("import('@/lib/screen-capture/tauri-capture-adapter')")
    expect(chatViewSource).toContain('target.activate(currentCaptureDestination())')
    expect(chatViewSource).toContain('!notAllowChat.value')
    expect(chatViewSource).toContain('watch([chatId, channelId, notAllowChat]')
    expect(chatViewSource).toContain('captureTarget?.deactivate()')
    expect(chatViewSource).toContain('captureTarget?.dispose()')
    expect(chatViewSource).toContain('doUploadImages([file], destination)')
    expect(tauriAdapterSource).not.toMatch(/from ['"]@tauri-apps/)
    expect(tauriAdapterSource).toContain("import('@tauri-apps/api/core')")
    expect(tauriAdapterSource).toContain('getCaptureClient({')
  })

  it('renders only in Tauri and emits a request without starting capture itself', async () => {
    const wrapper = mount(ChatInput, {
      props: { modelValue: '', createLoading: false },
      global: {
        mocks: { $t: (key: string) => key },
        directives: { tooltip: () => {} },
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
        },
      },
    })
    wrappers.push(wrapper)

    const button = wrapper.find('[data-testid="screen-capture-button"]')
    expect(button.exists()).toBe(__IS_TAURI__)
    if (__IS_TAURI__) {
      await button.trigger('click')
      expect(wrapper.emitted('request-capture')).toHaveLength(1)
    }
  })
})
