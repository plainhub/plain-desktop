import { afterEach, beforeEach, expect, it, vi } from 'vitest'

vi.mock('@/lib/device/client-id', () => ({
  getDesktopClientId: () => 'desktop',
  getNativeFileUrlToken: () => btoa('k'.repeat(32)),
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
  expect(proxyWsUrlFor('wss://device.example.test:8443/screen?cid=desktop', 'peer-1'))
    .toBe('ws://127.0.0.1:8280/proxy/peer-1/screen?cid=desktop')
})

it('keeps sockets without a peer id on the encrypted target ticket', () => {
  const url = new URL(proxyWsUrlFor('wss://device.example.test:8443/auth?cid=login&auth=1'))
  expect(url.pathname).toMatch(/^\/proxy\/c\.[^/]+\/auth$/)
  expect(url.search).toBe('?cid=login&auth=1')
})

it('connects loopback sockets directly', () => {
  expect(proxyWsUrlFor('ws://127.0.0.1:8280/screen?cid=desktop', 'peer-1')).toBe('ws://127.0.0.1:8280/screen?cid=desktop')
})
