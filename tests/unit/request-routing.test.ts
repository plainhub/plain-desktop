import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { getApiBaseUrl, getWebSocketBaseUrl, proxyHttpUrl, proxyWsUrlFor, setLocalServerPort, setUrlToken, setPendingLoginDevice, clearPendingLoginDevice } from '@/lib/api/api'
import { DeviceType } from '@/lib/status'

const target = vi.hoisted(() => ({ local: true, host: '', base: '' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => target.local }))
vi.mock('@/lib/device/current', () => ({ getCurrentDeviceHost: () => target.host, getCurrentDeviceBaseUrl: () => target.base }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => 'self' }))

beforeEach(() => {
  Object.assign(target, { local: true, host: '', base: '' })
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', { location: { protocol: 'http:', host: 'web.example:8080' } })
  vi.stubEnv('VITE_APP_API_HOST', '')
  clearPendingLoginDevice()
  setLocalServerPort(8180)
  setUrlToken(Buffer.alloc(32, 7).toString('base64'))
})
afterEach(() => { vi.unstubAllGlobals(); vi.unstubAllEnvs() })

it('uses loopback resources only for the local target', () => {
  expect(getApiBaseUrl()).toBe('http://localhost:8180')
  expect(getWebSocketBaseUrl()).toBe('ws://localhost:8180')
  target.local = false
  expect(getApiBaseUrl()).toBe('http://web.example:8080')
})

it('preserves the selected remote HTTPS base independently of the build flag', () => {
  target.local = false
  target.host = 'remote.example:8080'
  target.base = 'https://remote.example:8080'
  for (const desktop of [true, false]) {
    vi.stubGlobal('__IS_TAURI__', desktop)
    expect(getApiBaseUrl()).toBe(target.base)
    expect(getWebSocketBaseUrl()).toBe('wss://remote.example:8080')
  }
})

it('routes pending login to its own target and restores the selected device afterward', () => {
  target.base = 'https://selected.example:8443'
  setPendingLoginDevice({ host: 'pending.example:8443', deviceType: DeviceType.OTHER })
  expect(getApiBaseUrl()).toBe('https://pending.example:8443')
  expect(getWebSocketBaseUrl()).toBe('wss://pending.example:8443')
  clearPendingLoginDevice()
  expect(getApiBaseUrl()).toBe(target.base)
})

it('uses configured browser API hosts and preserves HTTPS on custom ports', () => {
  target.local = false
  vi.stubEnv('VITE_APP_API_HOST', 'configured.example:8043')
  expect(getApiBaseUrl()).toBe('https://configured.example:8043')
  vi.stubEnv('VITE_APP_API_HOST', '')
  window.location.protocol = 'https:'
  expect(getWebSocketBaseUrl()).toBe('wss://web.example:8080')
})

it('keeps production browser requests on the current origin despite development environment files', () => {
  target.local = false
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubEnv('DEV', false)
  vi.stubEnv('VITE_APP_API_HOST', 'development.example:8080')
  window.location.protocol = 'https:'
  expect(getApiBaseUrl()).toBe('https://web.example:8080')
  expect(getWebSocketBaseUrl()).toBe('wss://web.example:8080')
})

it('uses the existing Core listener for native HTTP', () => {
  const init = new URL(proxyHttpUrl('https://device.example:8443/init?value=a%2Bb'))
  expect(init.origin).toBe('http://127.0.0.1:8180')
  expect(init.pathname).toMatch(/^\/proxy\/c\.self\.[\w-]+\/init$/)
  expect(init.search).toBe('?value=a%2Bb')
  expect(proxyWsUrlFor('ws://localhost:8180/graphql')).toBe('ws://localhost:8180/graphql')
})

it('replaces cached native resource tickets when its stored session URL credential rotates', () => {
  const url = 'https://device.example:8443/init'
  const first = proxyHttpUrl(url)
  expect(proxyHttpUrl(url)).toBe(first)
  setUrlToken(Buffer.alloc(32, 8).toString('base64'))
  const rotated = proxyHttpUrl(url)
  expect(rotated).not.toBe(first)
  expect(proxyHttpUrl(url)).toBe(rotated)
})
