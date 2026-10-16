import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  vi.useFakeTimers()
  vi.setSystemTime(1000)
})
afterEach(() => vi.useRealTimers())

it('pins the document scope once and preserves ordinary GraphQL fields and extensions', async () => {
  fixture.invoke.mockImplementation(async (command: string) => command === 'native_view_scope' ? 'document' : { data: { ok: true } })
  const { nativeGraphql } = await import('../../src/lib/api/native-client')
  const body = { query: 'query Read($id: ID!) { node(id: $id) { id } }', variables: { id: 'item' }, operationName: 'Read', extensions: { trace: 'test', nativeScope: 'untrusted' } }
  await nativeGraphql(body, 500)
  await nativeGraphql({ query: '{ app { name } }' }, 500)
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_view_scope')).toHaveLength(1)
  expect(fixture.invoke.mock.calls[1]).toEqual(['native_graphql', { body: {
    ...body, extensions: { trace: 'test', nativeScope: 'document', nativeDeadlineMs: 1500 },
  } }])
  expect(vi.getTimerCount()).toBe(0)
})

it('does not issue a mutation when the document handshake completes after its deadline', async () => {
  let finish!: (scope: string) => void
  fixture.invoke.mockImplementation(() => new Promise<string>((resolve) => { finish = resolve }))
  const { nativeGraphql } = await import('../../src/lib/api/native-client')
  const request = nativeGraphql({ query: 'mutation { acquireDiscovery(token: "view") }' }, 500)
  const rejected = expect(request).rejects.toThrow('connection_timeout')
  await vi.advanceTimersByTimeAsync(500)
  await rejected
  finish('document')
  await Promise.resolve()
  await Promise.resolve()
  expect(fixture.invoke.mock.calls.map(([command]) => command)).toEqual(['native_view_scope'])
  expect(vi.getTimerCount()).toBe(0)
})

it('the deadline includes handshake time instead of restarting the timeout at dispatch', async () => {
  let finish!: (scope: string) => void
  fixture.invoke.mockImplementation((command: string) => command === 'native_view_scope'
    ? new Promise<string>((resolve) => { finish = resolve })
    : Promise.resolve({ data: {} }))
  const { nativeGraphql } = await import('../../src/lib/api/native-client')
  const request = nativeGraphql({ query: '{ app { name } }' }, 500)
  await vi.advanceTimersByTimeAsync(400)
  finish('document')
  await request
  expect(fixture.invoke.mock.calls[1][1].body.extensions.nativeDeadlineMs).toBe(1500)
  expect(vi.getTimerCount()).toBe(0)
})
