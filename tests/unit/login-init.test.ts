import { afterEach, beforeEach, expect, it, vi } from 'vitest'
const mock = vi.hoisted(() => ({ token: vi.fn(), identity: vi.fn(), secure: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/plugins/router', () => ({ default: {} }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: vi.fn(), getPendingLoginDevice: vi.fn(), clearPendingLoginDevice: vi.fn() }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: mock.token, clearCurrentSession: vi.fn() }))
vi.mock('@/lib/device/login-peers', () => ({ getBrowserIdentity: mock.identity, forgetBrowserIdentity: vi.fn(), findLoginPeer: vi.fn(), saveBrowserIdentity: vi.fn(), saveBrowserSession: vi.fn(), preloadLoginPeers: vi.fn(), peerHost: vi.fn() }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: vi.fn(), setRemoteClientId: vi.fn() }))
vi.mock('@/lib/api/http', () => ({ httpRequest: vi.fn() }))
vi.mock('@/lib/api/login-handshake', () => ({ performLoginHandshake: vi.fn(), secureLoginTransport: mock.secure }))
vi.mock('@/lib/device/pairing', () => ({ createPairing: vi.fn() }))
vi.mock('@/lib/prefs', () => ({ get: vi.fn() }))
import { clearCurrentSession } from '../../src/lib/device/current'
import { useLogin } from '../../src/views/login/login'

const data = { signaturePublicKey: 'trusted-key', authenticated: false, passwordRequired: false }
beforeEach(() => {
  vi.clearAllMocks()
  vi.stubGlobal('__IS_TAURI__', false)
  mock.secure.mockReturnValue(true)
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

it('rejects changed identities before auto-login and refuses insecure page login', () => {
  const success = vi.fn()
  mock.identity.mockReturnValue({ deviceId: 'device', publicKey: 'previous-key' })
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success })
  expect(login.applyInitResult({ status: 200, data: { ...data, authenticated: true } })).toBe(false)
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(login.error.value).toBe('login.peer_identity_changed')
  expect(login.webAccessDisabled.value).toBe(true)
  expect(success).not.toHaveBeenCalled()
  mock.identity.mockReturnValue(undefined)
  mock.secure.mockReturnValue(false)
  expect(login.applyInitResult({ status: 200, data })).toBe(false)
  expect(login.error.value).toBe('login.secure_connection_required')
})
