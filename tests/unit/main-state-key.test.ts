import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  remote: '',
  desktop: '706c6169-a25b-8d40-afed-280d7e045bff',
  browser: '706c6169-c4f7-8820-be62-6fba883a05ea',
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
  fixture.desktop = '706c6169-a25b-8d40-afed-280d7e045bff'
  fixture.browser = '706c6169-c4f7-8820-be62-6fba883a05ea'
  fixture.identity = { deviceId: '706c6169-3dee-8e04-918a-58062a09d734', publicKey: 'fixture-public-key' }
})
afterEach(() => vi.unstubAllGlobals())

it('keeps the server key after refreshing without a session binding', () => {
  fixture.remote = '706c6169-3dee-8e04-918a-58062a09d734'
  expect(getMainStateKey()).toBe('main_state:706c6169-3dee-8e04-918a-58062a09d734')
  fixture.remote = ''
  expect(getMainStateKey()).toBe('main_state:706c6169-3dee-8e04-918a-58062a09d734')
})

it('does not use the browser client ID when it changes', () => {
  fixture.browser = '706c6169-cff1-874d-98e0-889f191f631b'
  expect(getMainStateKey()).toBe('main_state:706c6169-3dee-8e04-918a-58062a09d734')
})

it('does not choose a key before the browser has a server identity', () => {
  fixture.identity = undefined
  expect(getMainStateKey()).toBeUndefined()
})

it('uses the confirmed browser identity after the server changes', () => {
  fixture.remote = '706c6169-3c85-88c9-84cf-24ac1ca78141'
  fixture.identity = { deviceId: '706c6169-2e24-8576-93db-19cc069edcda', publicKey: 'replacement-public-key' }
  expect(getMainStateKey()).toBe('main_state:706c6169-2e24-8576-93db-19cc069edcda')
})

it('isolates native local and remote targets', () => {
  vi.stubGlobal('__IS_TAURI__', true)
  expect(getMainStateKey()).toBe('main_state:706c6169-a25b-8d40-afed-280d7e045bff')
  fixture.remote = '706c6169-1a20-8671-ba46-0e75536697f0'
  expect(getMainStateKey()).toBe('main_state:706c6169-1a20-8671-ba46-0e75536697f0')
  fixture.remote = ''
  expect(getMainStateKey()).toBe('main_state:706c6169-a25b-8d40-afed-280d7e045bff')
})
