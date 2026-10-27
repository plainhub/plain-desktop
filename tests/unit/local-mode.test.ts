import { afterEach, expect, it, vi } from 'vitest'
import { isLocalMode } from '@/lib/device/local-mode'

const target = vi.hoisted(() => ({ remote: '' }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => target.remote }))
afterEach(() => vi.unstubAllGlobals())

it.each([[false, '', false], [false, 'peer', false], [true, '', true], [true, 'peer', false]])('selects local mode for Desktop=%s remote=%s', (desktop, remote, expected) => {
  vi.stubGlobal('__IS_TAURI__', desktop)
  target.remote = remote
  expect(isLocalMode()).toBe(expected)
})
