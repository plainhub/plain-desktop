import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { h } from 'vue'
import LoginForm from '@/views/login/LoginForm.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import RequestVerificationCode from '@/components/RequestVerificationCode.vue'
import RequestApproval from '@/components/RequestApproval.vue'
import { pairingViews } from '@/lib/device/pairing'
import chat from '@/locales/en-US/chat'
import { click, clientTemplate, content, flush, nodes, renderView, type Node } from '../helpers/vue-node-renderer'

const io = vi.hoisted(() => ({ request: vi.fn(), listeners: new Map<string, (value: unknown) => void>() }))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: () => io.request }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: vi.fn() }))
vi.mock('@/lib/api/subscription-client', () => ({
  graphqlSubscriptions: {
    subscribe: (name: string, callback: (value: unknown) => void) => {
      io.listeners.set(name, callback)
      return () => io.listeners.delete(name)
    },
  },
}))
vi.mock('@/lib/api/api', () => ({ getPendingLoginDevice: () => undefined }))
vi.mock('@/lib/device/current', () => ({}))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: () => undefined }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => '' }))
vi.mock('@/lib/api/login-handshake', () => ({}))
vi.mock('@/plugins/router', () => ({ default: {} }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

clientTemplate(LoginForm, 'src/views/login/LoginForm.vue')
clientTemplate(VFilledButton, 'src/components/base/VFilledButton.vue')
clientTemplate(VOutlinedButton, 'src/components/base/VOutlinedButton.vue')
clientTemplate(RequestVerificationCode, 'src/components/RequestVerificationCode.vue')
clientTemplate(RequestApproval, 'src/components/RequestApproval.vue')

beforeEach(() => {
  vi.stubGlobal('__IS_TAURI__', false)
  io.listeners.clear()
  io.request.mockReset().mockResolvedValue({ data: { pairPeer: true, respondToPairing: true, cancelPairing: true } })
  pairingViews.clear()
})
afterEach(() => vi.unstubAllGlobals())

async function mountPairing() {
  const onCancel = vi.fn()
  const onSuccess = vi.fn()
  const empty = { render: () => null }
  let form: { startPairing(): Promise<void>; cancel(): void; hasCancel: boolean } | null = null
  const wrapper = { render: () => h(LoginForm, {
    ref: (vm: unknown) => { form = vm as typeof form },
    pairing: true, managedDeviceId: 'target', redirectOnSuccess: false, onCancel, onSuccess,
  }) }
  const view = renderView(wrapper, {}, {
    VFilledButton, VOutlinedButton, VCircularProgress: empty, VTextField: empty,
    TouchPhone: { render: () => h('svg', { id: 'tap-phone-fixture' }) },
  })
  const emit = (event: string, data: object) => {
    io.listeners.get(event)!(data)
    return flush()
  }
  const visible = (node?: Node): boolean => !node || node.style.display !== 'none' && visible(node.parent)
  const button = (label: string) => nodes(view.root).find(node => node.tag === 'button' && visible(node) && content(node) === label)
  await form!.startPairing()
  await flush()
  return { ...view, emit, button, onSuccess, onCancel, form: form!, text: () => nodes(view.root).filter(visible)
    .map(node => node.text ? chat[node.text as keyof typeof chat] ?? node.text : '').join(''),
    phone: () => nodes(view.root).some(node => visible(node) && node.props.id === 'tap-phone-fixture') }
}

it('shows the Tap Phone illustration only for actual remote approval, even without a code, and can cancel', async () => {
  const view = await mountPairing()
  try {
    expect(view.phone()).toBe(false)
    expect(view.text()).not.toContain('login.to_continue')
    await view.emit('pairingStarted', { deviceId: 'target', verificationCode: null })
    expect(view.phone()).toBe(true)
    expect(view.text()).toContain('login.to_continue')
    expect(view.text()).not.toContain('login.compare_code')
    click(view.button('cancel')!)
    await flush()
    expect(view.onCancel).toHaveBeenCalledOnce()
    expect(view.phone()).toBe(false)
    expect(io.request.mock.lastCall?.[1]).toEqual({ peerId: 'target' })
  } finally { view.unmount() }
})

it('waits for an explicit local trust decision, then displays remote approval and completes', async () => {
  const view = await mountPairing()
  try {
    await view.emit('pairingRequested', { fromId: 'target', verificationCode: '01234567', identityChanged: true })
    expect(view.phone()).toBe(false)
    expect(view.text()).toContain('no longer matches')
    expect(view.text()).toContain('Cancel if you didn’t expect')
    expect(view.text()).not.toContain('01234567')
    expect(view.text()).not.toContain('pairing_compare_code')
    expect(io.request).toHaveBeenCalledTimes(1)
    click(view.button('pairing_trust')!)
    await flush()
    expect(io.request.mock.lastCall?.[1]).toEqual({ peerId: 'target', accepted: true })
    expect(view.phone()).toBe(false)
    expect(view.text()).not.toContain('01234567')
    await view.emit('pairingStarted', { deviceId: 'target', verificationCode: '12345678' })
    expect(view.phone()).toBe(true)
    expect(view.text().match(/12345678/g)).toHaveLength(1)
    await view.emit('pairingCompleted', { deviceId: 'target' })
    expect(view.onSuccess).toHaveBeenCalledOnce()
    expect(view.phone()).toBe(false)
  } finally { view.unmount() }
})

it('shows the remote comparison code without local trust for a new managed device', async () => {
  const view = await mountPairing()
  try {
    expect(view.form.hasCancel).toBe(false)
    await view.emit('pairingStarted', { deviceId: 'target', verificationCode: '01234567' })
    expect(view.text()).toContain('01234567')
    expect(view.phone()).toBe(true)
    expect(view.button('pairing_trust')).toBeUndefined()
    expect(io.request).toHaveBeenCalledTimes(1)
    expect(view.form.hasCancel).toBe(true)
    view.form.cancel()
    await flush()
    expect(view.onCancel).toHaveBeenCalledOnce()
    expect(io.request.mock.lastCall?.[1]).toEqual({ peerId: 'target' })
  } finally { view.unmount() }
})
