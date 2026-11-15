import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  remote: '',
  desktop: 'local-device',
  browser: 'browser-client',
  identity: undefined as { deviceId: string; publicKey: string } | undefined,
}))
vi.mock('@/lib/device/client-id', () => ({
  getRemoteClientId: () => fixture.remote,
  getDesktopClientId: () => fixture.desktop,
  getActiveClientId: () => fixture.remote || (__IS_TAURI__ ? fixture.desktop : fixture.browser),
  clearRemoteClientId: vi.fn(),
}))
vi.mock('@/lib/device/login-peers', () => ({ getBrowserIdentity: () => fixture.identity }))
import { getMainStateKey } from '@/lib/device/current'

beforeEach(() => {
  vi.stubGlobal('__IS_TAURI__', false)
  fixture.remote = ''
  fixture.desktop = 'local-device'
  fixture.browser = 'browser-client'
  fixture.identity = { deviceId: 'server-device', publicKey: 'fixture-public-key' }
})
afterEach(() => vi.unstubAllGlobals())

it('keeps the server key after refreshing without a session binding', () => {
  fixture.remote = 'server-device'
  expect(getMainStateKey()).toBe('main_state:server-device')
  fixture.remote = ''
  expect(getMainStateKey()).toBe('main_state:server-device')
})

it('does not use the browser client ID when it changes', () => {
  fixture.browser = 'new-browser-client'
  expect(getMainStateKey()).toBe('main_state:server-device')
})

it('does not choose a key before the browser has a server identity', () => {
  fixture.identity = undefined
  expect(getMainStateKey()).toBeUndefined()
})

it('uses the confirmed browser identity after the server changes', () => {
  fixture.remote = 'previous-server'
  fixture.identity = { deviceId: 'replacement-server', publicKey: 'replacement-public-key' }
  expect(getMainStateKey()).toBe('main_state:replacement-server')
})

it('isolates native local and remote targets', () => {
  vi.stubGlobal('__IS_TAURI__', true)
  expect(getMainStateKey()).toBe('main_state:local-device')
  fixture.remote = 'remote-device'
  expect(getMainStateKey()).toBe('main_state:remote-device')
  fixture.remote = ''
  expect(getMainStateKey()).toBe('main_state:local-device')
})
