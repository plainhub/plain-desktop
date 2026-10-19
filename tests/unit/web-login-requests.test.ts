import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { randomBytes } from 'node:crypto'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { eventAad } from '../../src/lib/api/websocket-events'

const { subscribe, invoke } = vi.hoisted(() => ({ subscribe: vi.fn(), invoke: vi.fn() }))
vi.mock('@/lib/api/native-events', () => ({ subscribeNativeEvents: subscribe }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/lib/api/api', () => ({ getLocalToken: () => btoa(String.fromCharCode(...new Uint8Array(32).fill(8))) }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32).fill(8) }))
import { subscribeWebLoginRequests, decideWebLogin } from '../../src/lib/device/web-login-requests'

const request = { requestId: 'request', clientId: 'browser', clientIp: '127.0.0.1', request: { browserName: 'Fixture', browserVersion: '1', osName: 'Fixture', osVersion: '1', verificationCode: '01234567 89abcdef 01234567 89abcdef' } }

beforeEach(() => { vi.clearAllMocks(); vi.stubGlobal('__IS_TAURI__', true) })
afterEach(() => vi.unstubAllGlobals())

it('receives approval prompts only from authenticated local native event frames', () => {
  const receive = vi.fn()
  const dismiss = vi.fn()
  subscribeWebLoginRequests(receive, vi.fn(), dismiss)
  const callback = subscribe.mock.calls[0][0] as (frame: Uint8Array) => void
  const event = 'WEB_LOGIN_REQUESTED'
  const encrypt = (key: number) => {
    const nonce = randomBytes(24)
    return new Uint8Array([...nonce, ...xchacha20poly1305(new Uint8Array(32).fill(key), nonce, eventAad(event)).encrypt(new TextEncoder().encode(JSON.stringify(request)))])
  }
  const payload = encrypt(8)
  callback(new Uint8Array([...new TextEncoder().encode(`${event}\0`), ...payload]))
  expect(receive).toHaveBeenCalledExactlyOnceWith(request)
  callback(new Uint8Array([...new TextEncoder().encode('PAIRING_STATE_CHANGED\0'), ...payload]))
  expect(receive).toHaveBeenCalledTimes(1)
  const closedNonce = randomBytes(24)
  const closed = xchacha20poly1305(new Uint8Array(32).fill(8), closedNonce, eventAad('WEB_LOGIN_CLOSED')).encrypt(new TextEncoder().encode(JSON.stringify({ requestId: 'request' })))
  callback(new Uint8Array([...new TextEncoder().encode('WEB_LOGIN_CLOSED\0'), ...closedNonce, ...closed]))
  expect(dismiss).toHaveBeenCalledExactlyOnceWith('request')
  const forged = encrypt(9)
  expect(() => callback(new Uint8Array([...new TextEncoder().encode(`${event}\0`), ...forged]))).toThrow()
  expect(receive).toHaveBeenCalledTimes(1)
})

it('uses the local named confirmation command and refuses a Web event subscription', async () => {
  invoke.mockResolvedValue(true)
  await expect(decideWebLogin('request', false)).resolves.toBe(true)
  expect(invoke).toHaveBeenCalledExactlyOnceWith('native_confirm_web_login', { requestId: 'request', allowed: false })
  vi.stubGlobal('__IS_TAURI__', false)
  expect(() => subscribeWebLoginRequests(vi.fn(), vi.fn(), vi.fn())).toThrow('native_only')
  expect(subscribe).not.toHaveBeenCalled()
})
