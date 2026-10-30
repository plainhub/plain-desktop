import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const request = vi.hoisted(() => vi.fn())
vi.mock('@/lib/api/http', () => ({ httpRequest: request }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: () => 'https://example.test', getProxyUrl: (url: string) => url }))
vi.mock('@/lib/api/crypto', () => ({
  chachaEncrypt: () => new Uint8Array(),
  chachaDecrypt: vi.fn(),
  httpResponseAad: () => new Uint8Array(),
  arrayBufferToBitArray: vi.fn(),
  bitArrayToUint8Array: (value: Uint8Array) => value,
  bitArrayToBase64: vi.fn(),
}))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => value }))

import { guestFetch } from '@/lib/api/guest'

beforeEach(() => {
  vi.resetAllMocks()
  vi.useFakeTimers()
  vi.stubGlobal('window', { __PLAIN_LOG__: false })
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

it('retains the abort cause when reporting a guest request timeout', async () => {
  const cause = new DOMException('Aborted', 'AbortError')
  request.mockRejectedValue(cause)
  await expect(guestFetch('share', new Uint8Array(32), '{ sharedInfo { name } }')).rejects.toMatchObject({
    message: 'connection_timeout', cause,
  })
  expect(vi.getTimerCount()).toBe(0)
})

it('preserves other guest request errors and clears the timer', async () => {
  const error = new Error('network_failure')
  request.mockRejectedValue(error)
  await expect(guestFetch('share', new Uint8Array(32), '{ sharedInfo { name } }')).rejects.toBe(error)
  expect(vi.getTimerCount()).toBe(0)
})
