import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { gqlFetch, bindGraphqlClient } from '@/lib/api/gql-client'
import { gqlFetchPeer } from '@/lib/api/peer-client'
import type { LoginPeer } from '@/lib/device/login-peers'

const fixture = vi.hoisted(() => ({ invoke: vi.fn(), http: vi.fn(), key: vi.fn(), local: false, selected: '706c6169-4132-8ab2-8ec9-5a9f1a04658b' }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('@/lib/api/api', () => ({
  getApiBaseUrl: () => 'https://selected.example.test',
  getApiHeaders: () => ({ 'c-id': '706c6169-1ac2-825a-8c79-0951ec181d4a' }),
  deviceBaseUrl: (host: string) => `https://${host}`,
}))
vi.mock('@/lib/api/http', () => ({ httpRequest: fixture.http }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => fixture.local }))
vi.mock('@/lib/device/client-id', () => ({ getActiveClientId: () => fixture.selected || '706c6169-1ac2-825a-8c79-0951ec181d4a', getRemoteClientId: () => fixture.selected }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => 'selected-session', clearCurrentSession: vi.fn() }))
vi.mock('@/lib/device/login-peers', async original => ({ ...(await original<typeof import('@/lib/device/login-peers')>()), getLoginPeerRevision: () => 1 }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: fixture.key }))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => value }))
vi.mock('@/lib/api/crypto', async original => ({
  ...(await original<typeof import('@/lib/api/crypto')>()),

  chachaEncrypt: (_key: string, value: string) => new TextEncoder().encode(value),
  decryptGraphqlResponse: (_key: string, _request: unknown, value: ArrayBuffer) => new TextDecoder().decode(value),
  arrayBufferToBitArray: (value: unknown) => value,
  bitArrayToUint8Array: (value: unknown) => value,
}))

const peer = { id: '706c6169-d74d-885c-bc72-0b0704412cf1', token: 'direct-session', ip: 'direct.example.test', port: 443 } as LoginPeer
const query = 'query Read($id: ID!) { node(id: $id) { id } }'
const variables = { id: 'item' }
const result = { data: { node: { id: 'item' } } }

beforeEach(() => {
  vi.resetAllMocks()
  vi.useFakeTimers()
  vi.setSystemTime(1000)
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', { __PLAIN_LOG__: false })
  fixture.local = false
  fixture.selected = '706c6169-4132-8ab2-8ec9-5a9f1a04658b'
  fixture.key.mockImplementation((token: string) => token)
  fixture.invoke.mockImplementation(async (command: string) => command === 'native_view_scope' ? 'document' : result)
  fixture.http.mockResolvedValue({ status: 200, ok: true, arrayBuffer: async () => new TextEncoder().encode(JSON.stringify(result)).buffer })
})
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals() })

it('sends selected, direct and fixed local GraphQL through the same scoped IPC contract', async () => {
  expect(await gqlFetch(query, variables)).toEqual(result)
  expect(await gqlFetchPeer(peer, query, variables)).toEqual(result)
  fixture.local = true
  fixture.selected = ''
  expect(await gqlFetch(query, variables)).toEqual(result)
  const body = { query, variables, extensions: { nativeScope: 'document', nativeDeadlineMs: 31000 } }
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_graphql')).toEqual([
    ['native_graphql', { body, target: { kind: 'PEER', peerId: '706c6169-4132-8ab2-8ec9-5a9f1a04658b' } }],
    ['native_graphql', { body, target: { kind: 'PEER', peerId: '706c6169-d74d-885c-bc72-0b0704412cf1' } }],
    ['native_graphql', { body, target: { kind: 'LOCAL' } }],
  ])
  expect(fixture.http).not.toHaveBeenCalled()
  expect(fixture.key).not.toHaveBeenCalled()
})

it('pins a bound IPC target while later direct and current calls use their own peers', async () => {
  const bound = bindGraphqlClient()
  fixture.selected = '706c6169-a60b-8d45-b5e2-8d95a2b44a95'
  await bound(query, variables)
  await gqlFetchPeer(peer, query, variables)
  await gqlFetch(query, variables)
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_graphql').map(([, args]) => args.target)).toEqual([
    { kind: 'PEER', peerId: '706c6169-4132-8ab2-8ec9-5a9f1a04658b' }, { kind: 'PEER', peerId: '706c6169-d74d-885c-bc72-0b0704412cf1' }, { kind: 'PEER', peerId: '706c6169-a60b-8d45-b5e2-8d95a2b44a95' },
  ])
  expect(fixture.http).not.toHaveBeenCalled()
})

it('does not substitute the local client ID for a missing selected or direct peer ID', async () => {
  fixture.selected = ''
  await expect(gqlFetch(query, variables)).rejects.toThrow('NATIVE_PEER_ID_MISSING')
  await expect(gqlFetchPeer({ ...peer, id: '' }, query, variables)).rejects.toThrow('NATIVE_PEER_ID_MISSING')
  expect(fixture.invoke).not.toHaveBeenCalled()
  expect(fixture.http).not.toHaveBeenCalled()
})

it('does not fall back to HTTP after a Desktop direct-peer failure', async () => {
  fixture.invoke.mockImplementation(async (command: string) => {
    if (command === 'native_view_scope') return 'document'
    throw 'peer_unavailable'
  })
  await expect(gqlFetchPeer(peer, query, variables)).rejects.toThrow('peer_unavailable')
  expect(fixture.http).not.toHaveBeenCalled()
})

it('preserves browser direct-peer HTTPS and its request body', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  expect(await gqlFetchPeer(peer, query, variables)).toEqual(result)
  expect(fixture.http).toHaveBeenCalledExactlyOnceWith('https://direct.example.test/graphql', expect.objectContaining({
    method: 'POST', headers: { 'c-id': '706c6169-1ac2-825a-8c79-0951ec181d4a' }, body: expect.any(Uint8Array),
  }))
  expect(fixture.invoke).not.toHaveBeenCalled()
  const payload = JSON.parse(new TextDecoder().decode(fixture.http.mock.calls[0][1].body))
  expect(payload).toEqual({ query, variables, requestId: expect.stringMatching(/^00000000-03e8-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/) })
  expect(fixture.key).toHaveBeenCalledExactlyOnceWith('direct-session')
})
