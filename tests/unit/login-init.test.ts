import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'
const mock = vi.hoisted(() => ({ token: vi.fn(), identity: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/plugins/router', () => ({ default: {} }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: vi.fn(), getPendingLoginDevice: vi.fn(), clearPendingLoginDevice: vi.fn() }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: mock.token, clearCurrentSession: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@/lib/device/login-peers', () => ({ getBrowserIdentity: mock.identity, forgetBrowserIdentity: vi.fn(), findLoginPeer: vi.fn(), saveBrowserIdentity: vi.fn(), saveBrowserSession: vi.fn(), preloadLoginPeers: vi.fn(), peerHost: vi.fn() }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: vi.fn(), getRemoteClientId: vi.fn(), setRemoteClientId: vi.fn() }))
vi.mock('@/lib/api/http', () => ({ httpRequest: vi.fn() }))
vi.mock('@/lib/api/login-handshake', () => ({ performLoginHandshake: vi.fn() }))
vi.mock('@/lib/device/pairing', () => ({ createPairing: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: vi.fn() }))
import { createPairing, type PairingState } from '@/lib/device/pairing'
import { setRemoteClientId } from '@/lib/device/client-id'
import { saveBrowserIdentity, saveBrowserSession } from '@/lib/device/login-peers'
import { clearCurrentSession } from '../../src/lib/device/current'
import { performLoginHandshake } from '../../src/lib/api/login-handshake'
import { useLogin } from '../../src/views/login/login'

const data = { signaturePublicKey: 'trusted-key', authenticated: false, passwordRequired: false }
beforeEach(() => {
  vi.clearAllMocks()
  vi.stubGlobal('__IS_TAURI__', false)
  mock.identity.mockReturnValue(undefined)
  mock.token.mockReturnValue('stale-token')
})
afterEach(() => vi.unstubAllGlobals())

it('does not treat a missing password as acceptance of a stale stored token', () => {
  const success = vi.fn()
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success })
  expect(login.applyInitResult({ status: 200, data })).toBe(false)
  expect(success).not.toHaveBeenCalled()
  expect(clearCurrentSession).toHaveBeenCalledTimes(1)
  expect(login.password.value).toBe('')
  expect(login.showPasswordInput.value).toBe(false)
})

it('auto-authenticates only after explicit session acceptance', () => {
  const success = vi.fn()
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success })
  expect(login.applyInitResult({ status: 200, data: { ...data, authenticated: true } })).toBe(true)
  expect(success).toHaveBeenCalledTimes(1)
  expect(clearCurrentSession).not.toHaveBeenCalled()
})

it('shows the password input from the explicit fixed-password flag', () => {
  const login = useLogin({ redirectOnSuccess: false })
  login.password.value = 'old-autofill'
  expect(login.applyInitResult({ status: 200, data: { ...data, passwordRequired: true } })).toBe(false)
  expect(login.showPasswordInput.value).toBe(true)
  expect(login.password.value).toBe('')
})

it('rejects changed identities before auto-login', () => {
  const success = vi.fn()
  mock.identity.mockReturnValue({ deviceId: 'device', publicKey: 'previous-key' })
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success })
  expect(login.applyInitResult({ status: 200, data: { ...data, authenticated: true } })).toBe(false)
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(login.error.value).toBe('login.peer_identity_changed')
  expect(login.webAccessDisabled.value).toBe(true)
  expect(success).not.toHaveBeenCalled()
})

it('starts B management through Core and keeps the Web session on A', async () => {
  const state = ref<PairingState | null>(null)
  const start = vi.fn(async () => {
    state.value = { deviceId: 'device-b', generation: 'fixture', mode: 'MANAGE', phase: 'SUCCEEDED', error: null }
  })
  vi.mocked(createPairing).mockReturnValue({
    state, start, submit: vi.fn(), cancel: vi.fn(), dispose: vi.fn(),
  })
  const success = vi.fn()
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success, managedDeviceId: 'device-b' })
  await login.startPairing()
  await nextTick()
  expect(start).toHaveBeenCalledWith({ id: 'device-b', mode: 'MANAGE', password: undefined })
  expect(success).toHaveBeenCalledOnce()
  expect(setRemoteClientId).not.toHaveBeenCalled()
  expect(saveBrowserSession).not.toHaveBeenCalled()
})

it('rejects an OPAQUE key change before auto-login', () => {
  const success = vi.fn()
  mock.identity.mockReturnValue({ deviceId: 'device', publicKey: data.signaturePublicKey, opaquePublicKey: 'old-key' })
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success })
  login.applyInitResult({ status: 200, data: { ...data, passwordRequired: true, authenticated: true, opaquePublicKey: 'new-key' } })
  expect(login.error.value).toBe('login.peer_identity_changed')
  expect(login.webAccessDisabled.value).toBe(true)
  expect(success).not.toHaveBeenCalled()
})

it('passes the entered password only to the handshake and stores the authenticated pin after success', async () => {
  const login = useLogin({ redirectOnSuccess: false })
  login.applyInitResult({ status: 200, data: { ...data, passwordRequired: true, opaquePublicKey: 'init-key' } })
  expect(saveBrowserIdentity).not.toHaveBeenCalled()
  login.password.value = 'synthetic password'
  vi.mocked(performLoginHandshake).mockResolvedValueOnce({ clientId: 'device', token: 'token', signaturePublicKey: data.signaturePublicKey, opaquePublicKey: 'init-key', chatPaired: false })
  await login.onSubmit()
  expect(performLoginHandshake).toHaveBeenCalledWith(expect.objectContaining({ password: 'synthetic password', initOpaquePublicKey: 'init-key' }))
  expect(vi.mocked(performLoginHandshake).mock.calls[0][0]).not.toHaveProperty('passwordHash')
  expect(login.password.value).toBe('')
  expect(saveBrowserIdentity).toHaveBeenCalledWith('device', data.signaturePublicKey, 'init-key')
})
