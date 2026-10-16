import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const { native, remote } = vi.hoisted(() => ({ native: vi.fn(), remote: vi.fn() }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))

import { readLocalAddresses, readLocalServer, restartLocalServer } from '../../../src/lib/api/local-server'

const pending = { revision: 1, restarting: true, httpPort: 8080, httpsPort: 8443, error: '' }

beforeEach(() => vi.resetAllMocks())
afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
})

it('reads live listener ports, resource token and addresses from the local main schema', async () => {
  native.mockResolvedValueOnce({ data: { app: { httpPort: 8180, httpsPort: 8543, urlToken: 'local-token' } } })
    .mockResolvedValueOnce({ data: { discoveryReply: { ips: ['192.0.2.1'] } } })
  expect(await readLocalServer()).toEqual({ httpPort: 8180, httpsPort: 8543, urlToken: 'local-token' })
  expect(await readLocalAddresses()).toEqual(['192.0.2.1'])
  expect(native.mock.calls[0][0].query).toContain('app { httpPort httpsPort urlToken }')
  expect(native.mock.calls[1][0].query).toContain('discoveryReply { ips }')
  expect(remote).not.toHaveBeenCalled()
})

it('waits only while the accepted restart is pending and preserves actual fallback ports', async () => {
  vi.useFakeTimers()
  native.mockResolvedValueOnce({ data: { restartHttpServer: pending } })
    .mockResolvedValueOnce({ data: { httpServer: { ...pending, restarting: false, httpPort: 8280, httpsPort: 8643 } } })
  const request = restartLocalServer('http', 8080)
  await vi.advanceTimersByTimeAsync(0)
  expect(native).toHaveBeenCalledTimes(1)
  expect(native.mock.calls[0][0].variables).toEqual({ httpPort: 8080 })
  await vi.advanceTimersByTimeAsync(250)
  await expect(request).resolves.toMatchObject({ restarting: false, httpPort: 8280, httpsPort: 8643 })
  expect(native).toHaveBeenCalledTimes(2)
  expect(vi.getTimerCount()).toBe(0)
})

it('returns persisted operation errors with the still-live ports', async () => {
  native.mockResolvedValue({ data: { restartHttpServer: { ...pending, restarting: false, error: 'Invalid TLS identity' } } })
  expect(await restartLocalServer('https', 0)).toMatchObject({ httpPort: 8080, httpsPort: 8443, error: 'Invalid TLS identity' })
  expect(native).toHaveBeenCalledTimes(1)
})

it('rejects refused and malformed operations without beginning a polling loop', async () => {
  for (const response of [{ errors: [{ message: 'no_permission' }] }, {}, { data: {} }, { data: { restartHttpServer: { revision: 1 } } }]) {
    native.mockResolvedValueOnce(response)
    await expect(restartLocalServer('http', 0)).rejects.toThrow()
  }
  expect(native).toHaveBeenCalledTimes(4)
})

it('rejects a superseded restart instead of reporting another requests completion', async () => {
  vi.useFakeTimers()
  native.mockResolvedValueOnce({ data: { restartHttpServer: pending } })
    .mockResolvedValueOnce({ data: { httpServer: { ...pending, revision: 2, restarting: false } } })
  const result = expect(restartLocalServer('http', 0)).rejects.toThrow('superseded')
  await vi.advanceTimersByTimeAsync(250)
  await result
  expect(vi.getTimerCount()).toBe(0)
})

it('bounds a pending restart and leaves no polling timer behind', async () => {
  vi.useFakeTimers()
  native.mockResolvedValueOnce({ data: { restartHttpServer: pending } })
    .mockResolvedValue({ data: { httpServer: pending } })
  const result = expect(restartLocalServer('http', 0)).rejects.toThrow('connection_timeout')
  await vi.advanceTimersByTimeAsync(30000)
  await result
  expect(vi.getTimerCount()).toBe(0)
})
