import { afterEach, beforeEach, expect, it, vi } from 'vitest'

vi.mock('@/lib/device/client-id', () => ({
  getDesktopClientId: () => '706c6169-5ed8-8c8a-94d8-a721217a2db3',
}))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/device/current', () => ({ getCurrentDeviceHost: () => '', getCurrentDeviceBaseUrl: () => '' }))

import { proxyWsUrlFor, setLocalServerPort } from '../../src/lib/api/api'

beforeEach(() => {
  vi.stubGlobal('__IS_TAURI__', true)
  setLocalServerPort(8280)
})

afterEach(() => vi.unstubAllGlobals())

it('routes a known peer socket by peer id and keeps the client id in the query', () => {
  expect(proxyWsUrlFor('wss://device.example.test:8443/screen?cid=706c6169-5ed8-8c8a-94d8-a721217a2db3', 'peer-1'))
    .toBe('ws://127.0.0.1:8280/proxy/peer-1/screen?cid=706c6169-5ed8-8c8a-94d8-a721217a2db3')
})

it('leaves sockets without a peer id untouched', () => {
  expect(proxyWsUrlFor('wss://device.example.test:8443/auth?cid=706c6169-96b0-86aa-b0dc-65655fc73829')).toBe('wss://device.example.test:8443/auth?cid=706c6169-96b0-86aa-b0dc-65655fc73829')
})

it('connects loopback sockets directly', () => {
  expect(proxyWsUrlFor('ws://127.0.0.1:8280/screen?cid=706c6169-5ed8-8c8a-94d8-a721217a2db3', 'peer-1')).toBe('ws://127.0.0.1:8280/screen?cid=706c6169-5ed8-8c8a-94d8-a721217a2db3')
})
