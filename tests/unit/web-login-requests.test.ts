import { beforeEach, expect, it, vi } from 'vitest'

const { invoke, snapshot, handlers } = vi.hoisted(() => ({
  invoke: vi.fn(),
  snapshot: vi.fn(),
  handlers: new Map<string, (event: { payload: any }) => void>(),
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class { onmessage?: (payload: any) => void },
}))

beforeEach(() => {
  vi.resetModules()
  vi.clearAllMocks()
  handlers.clear()
  vi.stubGlobal('__IS_TAURI__', true)
  snapshot.mockResolvedValue({ requests: [], nextAfter: null })
  invoke.mockImplementation(async (name, args) => {
    if (name === 'native_register_login_callbacks') {
      handlers.set('systemWebLoginRequested', ({ payload }) => args.requested.onmessage(payload))
      handlers.set('systemWebLoginClosed', ({ payload }) => args.closed.onmessage(payload))
      handlers.set('systemWebLoginSnapshot', ({ payload }) => args.snapshot.onmessage(payload))
      return 1
    }
    if (name === 'native_web_login_requests') return snapshot()
    return true
  })
})

it('restores pending logins and replays callbacks received during the snapshot', async () => {
  let resolveSnapshot!: (value: unknown) => void
  snapshot.mockReturnValue(new Promise((resolve) => { resolveSnapshot = resolve }))
  const receive = vi.fn()
  const dismiss = vi.fn()
  const { subscribeWebLoginRequests } = await import('../../src/lib/device/web-login-requests')
  const stream = subscribeWebLoginRequests(receive, vi.fn(), dismiss)
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith('native_web_login_requests', { after: null }))
  handlers.get('systemWebLoginClosed')!({ payload: { requestId: 'old' } })
  handlers.get('systemWebLoginRequested')!({ payload: { requestId: 'new', clientId: 'client', request: {} } })
  resolveSnapshot({ requests: [{ requestId: 'old', clientId: 'client', clientIp: '', browserName: '', browserVersion: '', osName: '', osVersion: '' }], nextAfter: null })
  await vi.waitFor(() => expect(receive).toHaveBeenCalledTimes(2))
  expect(dismiss).toHaveBeenCalledWith('old')
  expect(receive.mock.calls[1][0].requestId).toBe('new')
  expect(receive.mock.invocationCallOrder[0]).toBeLessThan(dismiss.mock.invocationCallOrder[0])
  expect(dismiss.mock.invocationCallOrder[0]).toBeLessThan(receive.mock.invocationCallOrder[1])
  stream.close()
  expect(invoke).toHaveBeenCalledWith('native_unregister_login_callbacks', { id: 1 })
})

it('uses the native confirmation command and rejects use in a web target', async () => {
  invoke.mockResolvedValue(true)
  const { subscribeWebLoginRequests, decideWebLogin } = await import('../../src/lib/device/web-login-requests')
  await expect(decideWebLogin('request', false)).resolves.toBe(true)
  expect(invoke).toHaveBeenCalledWith('native_confirm_web_login', { requestId: 'request', allowed: false })
  vi.stubGlobal('__IS_TAURI__', false)
  expect(() => subscribeWebLoginRequests(vi.fn(), vi.fn(), vi.fn())).toThrow('native_only')
})

it('replaces stale prompts when the host reconnects', async () => {
  snapshot.mockResolvedValue({ requests: [], nextAfter: null })
  const receive = vi.fn()
  const clear = vi.fn()
  const { subscribeWebLoginRequests } = await import('../../src/lib/device/web-login-requests')
  const stream = subscribeWebLoginRequests(receive, clear, vi.fn())
  await vi.waitFor(() => expect(invoke).toHaveBeenCalled())
  await vi.waitFor(() => expect(handlers.has('systemWebLoginRequested')).toBe(true))
  handlers.get('systemWebLoginSnapshot')!({ payload: { requests: [{ requestId: 'restored', clientId: 'browser', clientIp: '', request: {} }] } })
  await vi.waitFor(() => expect(receive).toHaveBeenCalledTimes(1))
  expect(clear).toHaveBeenCalledTimes(1)
  expect(receive.mock.calls[0][0].requestId).toBe('restored')
  stream.close()
})
