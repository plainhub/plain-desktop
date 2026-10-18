import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const { invoke, native, request, remote } = vi.hoisted(() => ({ invoke: vi.fn(), native: vi.fn(), request: vi.fn(), remote: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@/lib/api/http', () => ({ httpRequest: request }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))

import { readLocalAddresses, readLocalServer, restartLocalServer } from '../../src/lib/api/local-server'

const completed = { revision: 1, restarting: false, httpPort: 8080, httpsPort: 8443, error: '' }

beforeEach(() => vi.resetAllMocks())
afterEach(() => vi.useRealTimers())

it('restarts through native IPC and checks health at the actual returned HTTP port', async () => {
  invoke.mockResolvedValue({ ...completed, httpPort: 8180 })
  request.mockResolvedValue(new Response('OK'))
  expect(await restartLocalServer('https', 8443)).toEqual({ ...completed, httpPort: 8180 })
  expect(invoke).toHaveBeenCalledWith('native_restart_http_server', { httpsPort: 8443 })
  expect(request.mock.calls[0][0]).toBe('http://127.0.0.1:8180/health')
  expect(native).not.toHaveBeenCalled()
})

it('does not accept an old listener health response before native restart completes', async () => {
  let finish!: (value: typeof completed) => void
  invoke.mockReturnValue(new Promise((resolve) => { finish = resolve }))
  request.mockResolvedValue(new Response('OK'))
  const restarting = restartLocalServer('http', 8080)
  expect(request).not.toHaveBeenCalled()
  finish(completed)
  await restarting
  expect(request).toHaveBeenCalledTimes(1)
})

it('retries unsuccessful health responses instead of reporting success', async () => {
  vi.useFakeTimers()
  invoke.mockResolvedValue(completed)
  request.mockResolvedValueOnce(new Response('not ready', { status: 503 }))
    .mockResolvedValueOnce(new Response('OK'))
  const restarting = restartLocalServer('http', 8080)
  await vi.advanceTimersByTimeAsync(250)
  expect(await restarting).toEqual(completed)
  expect(request).toHaveBeenCalledTimes(2)
})

it('does not check health after native restart fails or returns an incomplete state', async () => {
  for (const response of [
    { ...completed, restarting: true },
    { ...completed, httpPort: 0 },
  ]) {
    invoke.mockResolvedValue(response)
    await expect(restartLocalServer('http', 8080)).rejects.toThrow()
  }
  expect(request).not.toHaveBeenCalled()
})

it('reads live listener ports, resource token and addresses from the local main schema', async () => {
  native.mockResolvedValueOnce({ data: { app: { httpPort: 8180, httpsPort: 8543, urlToken: 'local-token' } } })
    .mockResolvedValueOnce({ data: { app: { network: { ipv4Addresses: ['192.0.2.1'] } } } })
  expect(await readLocalServer()).toEqual({ httpPort: 8180, httpsPort: 8543, urlToken: 'local-token' })
  expect(await readLocalAddresses()).toEqual(['192.0.2.1'])
  expect(native.mock.calls[0][0].query).toContain('app { httpPort httpsPort urlToken }')
  expect(native.mock.calls[1][0].query).toContain('app { network { ipv4Addresses } }')
  expect(remote).not.toHaveBeenCalled()
})


it('preserves still-live ports and the restart error for the settings page', async () => {
  const failed = { ...completed, error: 'TLS identity invalid' }
  invoke.mockResolvedValue(failed)
  expect(await restartLocalServer('https', 0)).toEqual(failed)
  expect(request).not.toHaveBeenCalled()
})

it('propagates a superseded native restart without checking another operation health', async () => {
  invoke.mockRejectedValue(new Error('Server configuration was superseded'))
  await expect(restartLocalServer('http', 0)).rejects.toThrow('superseded')
  expect(request).not.toHaveBeenCalled()
})

it('bounds health retries and leaves no polling timer behind', async () => {
  vi.useFakeTimers()
  invoke.mockResolvedValue(completed)
  request.mockImplementation(async () => new Response('unavailable', { status: 503 }))
  const result = expect(restartLocalServer('http', 8080)).rejects.toThrow('connection_timeout')
  await vi.advanceTimersByTimeAsync(30000)
  await result
  expect(vi.getTimerCount()).toBe(0)
})
