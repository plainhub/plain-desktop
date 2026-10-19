import { createHash } from 'node:crypto'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({
  mounted: [] as (() => void)[],
  emit: vi.fn(),
  token: vi.fn(),
  socket: { binaryType: 'blob', send: vi.fn(), close: vi.fn(), onopen: null as null | (() => void), onclose: null as null | (() => void), onmessage: null as null | ((event: MessageEvent) => void) },
}))
vi.mock('vue', async (original) => ({ ...await original<typeof import('vue')>(), onMounted: (fn: () => void) => mock.mounted.push(fn), onUnmounted: () => {} }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (value: string) => value }) }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: mock.emit, on: vi.fn() } }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://example.test', getLocalToken: () => 'fixture', isNativeTarget: () => false, setLocalServerPort: vi.fn(), setLocalServerHttpsPort: vi.fn() }))
vi.mock('@/lib/peer/peer-events', () => ({ projectPeerManagementEvent: vi.fn(), projectPeerNotificationEvent: vi.fn() }))
vi.mock('@/lib/device/login-peers', () => ({ subscribeLoginPeerUpdates: vi.fn() }))
vi.mock('@/lib/theme', () => ({ applyDarkClass: vi.fn(), changeColor: vi.fn(), changeColorMode: vi.fn(), getCurrentMode: () => 'light', getLastSavedAutoColorMode: () => 'light', isModeDark: () => false }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(32).fill(7) }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => 'peer' }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: mock.token }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/http', () => ({ openSocket: vi.fn(() => mock.socket) }))
vi.mock('@/lib/api/native-events', () => ({ subscribeNativeEvents: vi.fn() }))
vi.mock('@/lib/prefs', () => ({ get: (_key: string, fallback: unknown) => fallback, set: vi.fn() }))

import { useAppSocket } from '../../src/hooks/app-socket'
import { openSocket } from '../../src/lib/api/http'
import { chachaDecrypt } from '../../src/lib/api/crypto'

function eventFrame(registration: Uint8Array, counter: bigint, payload = '{"probe":true}') {
    const sequence = Buffer.alloc(8)
    sequence.writeBigUInt64BE(counter)
    const name = 'CONTENT_CHANGED'
    const aad = Buffer.concat([Buffer.from('plain-ws-event-v1\0'), createHash('sha256').update(registration).digest(), sequence, Buffer.from(name)])
    const nonce = new Uint8Array(24).fill(8 + Number(counter))
    const encrypted = xchacha20poly1305(new Uint8Array(32).fill(7), nonce, aad).encrypt(Buffer.from(payload))
    return new Uint8Array(Buffer.concat([Buffer.from(name + '\0'), sequence, nonce, encrypted])).buffer
  }


beforeEach(() => {
  vi.clearAllMocks()
  mock.mounted.length = 0
  mock.token.mockReturnValue('fixture')
  vi.useFakeTimers()
  vi.stubGlobal('document', { title: '' })
  vi.stubGlobal('window', { matchMedia: () => ({ addEventListener: vi.fn() }) })
  vi.stubGlobal('__IS_TAURI__', false)
  vi.spyOn(console, 'error').mockImplementation(() => {})
})
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks() })

it('creates fresh registration only when the socket opens after a long connection delay', () => {
  vi.setSystemTime(1700000000000)
  useAppSocket()
  mock.mounted[0]!()
  expect(mock.socket.binaryType).toBe('arraybuffer')
  expect(mock.socket.send).not.toHaveBeenCalled()
  vi.setSystemTime(1700000123456)
  mock.socket.onopen!()
  expect(chachaDecrypt(new Uint8Array(32).fill(7), mock.socket.send.mock.calls[0]![0])).toBe('1700000123456')
})

it('dispatches authenticated events synchronously and closes only that socket on replay', () => {
  useAppSocket()
  mock.mounted[0]!()
  mock.socket.onopen!()
  mock.emit.mockClear()
  const registration = mock.socket.send.mock.calls[0]![0]
  const frame = eventFrame(registration, 0n)
  mock.socket.onmessage!({ data: frame } as MessageEvent)
  expect(mock.emit).toHaveBeenCalledExactlyOnceWith('CONTENT_CHANGED', { probe: true })
  mock.socket.onmessage!({ data: frame } as MessageEvent)
  expect(mock.emit).toHaveBeenCalledTimes(1)
  expect(mock.socket.close).toHaveBeenCalledExactlyOnceWith(1008, 'invalid_event')
  mock.socket.onmessage!({ data: eventFrame(registration, 1n) } as MessageEvent)
  expect(mock.emit).toHaveBeenCalledTimes(1)
})

it('keeps transport admission separate from an authenticated invalid presentation payload', () => {
  useAppSocket()
  mock.mounted[0]!()
  mock.socket.onopen!()
  mock.emit.mockClear()
  const registration = mock.socket.send.mock.calls[0]![0]
  mock.socket.onmessage!({ data: eventFrame(registration, 0n, 'not json') } as MessageEvent)
  expect(mock.socket.close).not.toHaveBeenCalled()
  expect(mock.emit).not.toHaveBeenCalled()
  mock.socket.onmessage!({ data: eventFrame(registration, 1n) } as MessageEvent)
  expect(mock.emit).toHaveBeenCalledExactlyOnceWith('CONTENT_CHANGED', { probe: true })
})

it('clears the disconnected state and stops retrying when initialization rejects the saved session', async () => {
  const state = useAppSocket()
  mock.mounted[0]!()
  expect(openSocket).toHaveBeenCalledTimes(1)
  mock.socket.onclose!()
  expect(state.wsStatus.value).toBe('closed')
  mock.token.mockReturnValue('')
  await vi.advanceTimersByTimeAsync(1000)
  expect(state.wsStatus.value).toBe('')
  await vi.advanceTimersByTimeAsync(5000)
  expect(openSocket).toHaveBeenCalledTimes(1)
})
