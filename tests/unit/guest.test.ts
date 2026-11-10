import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { ref } from 'vue'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
const request = vi.hoisted(() => vi.fn())
vi.mock('@/lib/api/http', () => ({ httpRequest: request }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: () => 'https://example.test', getProxyUrl: (url: string) => url }))
vi.mock('@/lib/api/crypto', () => ({ httpRequestAad: (path: string) => new TextEncoder().encode(`http-request\0POST\0${path}`),
  chachaEncrypt: () => new Uint8Array(),
  chachaDecrypt: vi.fn(),
  httpResponseAad: () => new Uint8Array(),
  arrayBufferToBitArray: vi.fn(),
  bitArrayToUint8Array: (value: Uint8Array) => value,
  bitArrayToBase64: vi.fn(),
}))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => value }))

import { guestFetch } from '@/lib/api/guest'
import { useShareData } from '@/views/share/hooks/useShareData'

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

it.each(['', '%%invalid%%', btoa('x'.repeat(31)), btoa('x'.repeat(33))])('rejects a malformed share token before any request (%s)', async token => {
  const share = useShareData(ref('owned-share'), ref(token))
  await expect(share.load('')).resolves.toBeUndefined()
  expect(share.state.value).toBe('error')
  expect(share.errorCode.value).toBe('invalid_link')
  expect(share.entries.value).toEqual([])
  expect(request).not.toHaveBeenCalled()
})

it('preserves the unauthorized state for a well-formed but rejected share key', async () => {
  request.mockResolvedValue(new Response('', { status: 401 }))
  const share = useShareData(ref('owned-share'), ref(btoa('x'.repeat(32))))
  await share.load('')
  expect(share.state.value).toBe('error')
  expect(share.errorCode.value).toBe('unauthorized')
  expect(share.entries.value).toEqual([])
  expect(request).toHaveBeenCalledTimes(1)
})
