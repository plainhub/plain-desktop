import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { DeviceType } from '@/lib/status'

const mocks = vi.hoisted(() => ({
  clientId: 'previous-device',
  init: vi.fn(),
  requestInit: vi.fn(),
  setPending: vi.fn(),
  clearPending: vi.fn(),
}))

vi.mock('@/views/login/LoginForm.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return {
    default: defineComponent({
      name: 'LoginForm',
      setup(_, { emit, expose }) {
        expose({ init: mocks.init })
        return () => h('button', { onClick: () => emit('success') }, 'complete')
      },
    }),
  }
})

vi.mock('@/lib/api/api', () => ({
  setPendingLoginDevice: mocks.setPending,
  clearPendingLoginDevice: mocks.clearPending,
}))

vi.mock('@/lib/api/init', () => ({ requestInit: mocks.requestInit }))

vi.mock('@/lib/device/client-id', () => ({
  getRemoteClientId: () => mocks.clientId,
  clearRemoteClientId: () => { mocks.clientId = '' },
  setRemoteClientId: (id: string) => { mocks.clientId = id },
}))

import DiscoveredDeviceLogin from '@/views/login/DiscoveredDeviceLogin.vue'

const device = { name: 'Pixel', host: '192.0.2.5:8443', deviceType: DeviceType.PHONE }

beforeEach(() => {
  mocks.clientId = 'previous-device'
  mocks.init.mockReset()
  mocks.requestInit.mockReset().mockResolvedValue({ status: 200, data: { password: 'temporary' } })
  mocks.setPending.mockReset()
  mocks.clearPending.mockReset()
})

describe('DiscoveredDeviceLogin', () => {
  it('reuses LoginForm auto-submit and restores the previous device on cancel', async () => {
    const wrapper = mount(DiscoveredDeviceLogin, {
      props: { device },
      global: { mocks: { $t: (key: string) => key } },
    })
    await flushPromises()
    expect(mocks.setPending).toHaveBeenCalledWith(device)
    expect(mocks.init).toHaveBeenCalledWith(
      { status: 200, data: { password: 'temporary' } },
      { autoSubmitWhenNoPassword: true },
    )
    expect(mocks.clientId).toBe('')
    wrapper.unmount()
    expect(mocks.clearPending).toHaveBeenCalledOnce()
    expect(mocks.clientId).toBe('previous-device')
  })

  it('keeps the newly logged-in device when login succeeds', async () => {
    const wrapper = mount(DiscoveredDeviceLogin, {
      props: { device },
      global: { mocks: { $t: (key: string) => key } },
    })
    await flushPromises()
    mocks.clientId = 'pixel-client'
    await wrapper.find('button').trigger('click')
    expect(wrapper.emitted('success')).toHaveLength(1)
    wrapper.unmount()
    expect(mocks.clientId).toBe('pixel-client')
  })
})
