import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { h } from 'vue'
import { CaptureClientError } from '@/lib/screen-capture/capture-client'
import { openScreenCapturePermissionSettings, requestScreenCapturePermission } from '@/lib/screen-capture/capture-permission'
import CapturePermissionModal from '@/views/screen-capture/CapturePermissionModal.vue'
import { isScreenCapturePermissionDenied, presentCaptureError } from '@/views/screen-capture/capture-error-presentation'

const mocks = vi.hoisted(() => ({
  currentModal: undefined as { component: unknown } | undefined,
  isMacPlatform: vi.fn(() => true),
  openModal: vi.fn(async () => ({})),
  popModal: vi.fn(async () => undefined),
  toast: vi.fn(),
}))

vi.mock('@/components/modal', () => ({
  getCurrentModal: () => mocks.currentModal,
  openModal: mocks.openModal,
  popModal: mocks.popModal,
}))
vi.mock('@/components/toaster', () => ({ default: mocks.toast }))
vi.mock('@/lib/platform', () => ({ isMacPlatform: mocks.isMacPlatform }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

const wrappers: Array<ReturnType<typeof mount>> = []
const t = ((key: string) => key) as any

beforeEach(() => {
  mocks.currentModal = undefined
  vi.clearAllMocks()
  mocks.isMacPlatform.mockReturnValue(true)
})

afterEach(() => {
  while (wrappers.length) wrappers.pop()!.unmount()
})

describe('screen capture permission presentation', () => {
  it('recognizes typed, native, and wrapped permission errors', () => {
    expect(isScreenCapturePermissionDenied(new CaptureClientError('permission_denied', 'required'))).toBe(true)
    expect(isScreenCapturePermissionDenied(new CaptureClientError('invalid_start', 'failed'))).toBe(false)
    expect(isScreenCapturePermissionDenied({ code: 'permission_denied' })).toBe(true)
    expect(isScreenCapturePermissionDenied(new Error('start failed', { cause: { code: 'permission_denied' } }))).toBe(true)
  })

  it('opens one guide for concurrent macOS permission failures', async () => {
    const error = new CaptureClientError('permission_denied', 'required')

    await Promise.all([presentCaptureError(error, t), presentCaptureError(error, t)])

    expect(mocks.openModal).toHaveBeenCalledOnce()
    expect(mocks.openModal).toHaveBeenCalledWith(CapturePermissionModal)
    expect(mocks.toast).not.toHaveBeenCalled()
  })

  it('keeps unrelated and non-macOS failures on the normal toast path', async () => {
    await presentCaptureError(new CaptureClientError('capture_busy', 'busy'), t)
    mocks.isMacPlatform.mockReturnValue(false)
    await presentCaptureError(new CaptureClientError('permission_denied', 'required'), t)

    expect(mocks.openModal).not.toHaveBeenCalled()
    expect(mocks.toast).toHaveBeenCalledTimes(2)
    expect(mocks.toast).toHaveBeenCalledWith('failed', 'error')
  })
})

describe('CapturePermissionModal', () => {
  it('requests native permission only after the guide action is clicked', async () => {
    const invoke = vi.fn(async () => undefined)
    await requestScreenCapturePermission(invoke)
    expect(invoke).toHaveBeenCalledOnce()
    expect(invoke).toHaveBeenCalledWith('screen_capture_request_permission')

    invoke.mockClear()
    await openScreenCapturePermissionSettings(invoke)
    expect(invoke).toHaveBeenCalledOnce()
    expect(invoke).toHaveBeenCalledWith('screen_capture_open_permission_settings')

    const requestPermission = vi.fn(async () => false)
    const openPermissionSettings = vi.fn(async () => undefined)
    const wrapper = mount(CapturePermissionModal, {
      props: { requestPermission, openPermissionSettings },
      global: {
        mocks: { $t: (key: string) => key },
        stubs: {
          VModal: {
            setup(_props: unknown, { slots }: any) {
              return () => h('div', [slots.headline?.(), slots.content?.(), slots.actions?.()])
            },
          },
          VOutlinedButton: {
            emits: ['click'],
            setup(_props: unknown, { emit, slots }: any) {
              return () => h('button', { onClick: () => emit('click') }, slots.default?.())
            },
          },
          VFilledButton: {
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

    expect(requestPermission).not.toHaveBeenCalled()
    expect(openPermissionSettings).not.toHaveBeenCalled()

    await wrapper.get('[data-testid="grant-screen-capture-permission"]').trigger('click')
    await flushPromises()

    expect(requestPermission).toHaveBeenCalledOnce()
    expect(openPermissionSettings).not.toHaveBeenCalled()

    await wrapper.get('[data-testid="grant-screen-capture-permission"]').trigger('click')
    await flushPromises()

    expect(openPermissionSettings).toHaveBeenCalledOnce()
  })

  it('closes the guide when macOS grants the request', async () => {
    const requestPermission = vi.fn(async () => true)
    const wrapper = mount(CapturePermissionModal, {
      props: { requestPermission },
      global: {
        mocks: { $t: (key: string) => key },
        stubs: {
          VModal: {
            setup(_props: unknown, { slots }: any) {
              return () => h('div', [slots.headline?.(), slots.content?.(), slots.actions?.()])
            },
          },
          VOutlinedButton: {
            emits: ['click'],
            setup(_props: unknown, { emit, slots }: any) {
              return () => h('button', { onClick: () => emit('click') }, slots.default?.())
            },
          },
          VFilledButton: {
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

    await wrapper.get('[data-testid="grant-screen-capture-permission"]').trigger('click')
    await flushPromises()

    expect(requestPermission).toHaveBeenCalledOnce()
    expect(mocks.popModal).toHaveBeenCalledOnce()
  })
})
