import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { getApiBaseUrl, getWebSocketBaseUrl, setLocalServerPort, setPendingLoginDevice, clearPendingLoginDevice } from '@/lib/api/api'
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
