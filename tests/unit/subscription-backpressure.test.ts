import { afterEach, expect, it, vi } from 'vitest'
const mock = vi.hoisted(() => ({ callbacks: null as any, send: vi.fn(), close: vi.fn() }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/subscription-connection', () => ({ openSubscriptionConnection: (callbacks: unknown) => {
  mock.callbacks = callbacks
  return { send: mock.send, close: mock.close }
} }))
import { SubscriptionClient } from '@/lib/api/subscription-client'
afterEach(() => { vi.restoreAllMocks(); vi.clearAllMocks() })
it('does not reattach existing consumers when another operation starts on the same connection', () => {
  const client = new SubscriptionClient()
  client.setOperations([
    { field: 'contentChanged', document: 'subscription { contentChanged }', access: [] },
    { field: 'mediaLibraryUpdated', document: 'subscription { mediaLibraryUpdated { types } }', access: [] },
  ])
  const attached = vi.fn()
  client.onAttached(attached)
  client.subscribe('contentChanged', vi.fn())
  client.start()
  mock.callbacks.opened()
  const first = mock.send.mock.calls.at(-1)![0]
  mock.callbacks.message({ id: first.id, type: 'next', payload: { extensions: { subscriptionReady: true } } })
  expect(attached).toHaveBeenCalledTimes(1)
  client.subscribe('mediaLibraryUpdated', vi.fn())
  const second = mock.send.mock.calls.at(-1)![0]
  mock.callbacks.message({ id: second.id, type: 'next', payload: { extensions: { subscriptionReady: true } } })
  expect(attached).toHaveBeenCalledTimes(1)
  client.stop()
})
it('resynchronizes only the lagged operation and preserves the socket and other consumers', () => {
  vi.spyOn(console, 'error').mockImplementation(() => {})
  const client = new SubscriptionClient()
  client.setOperations([
    { field: 'contentChanged', document: 'subscription { contentChanged }', access: [] },
    { field: 'mediaLibraryUpdated', document: 'subscription { mediaLibraryUpdated { types } }', access: [] },
  ])
  const content = vi.fn(), media = vi.fn(), attached = vi.fn(), status = vi.fn()
  client.subscribe('contentChanged', content)
  const stopMedia = client.subscribe('mediaLibraryUpdated', media)
  client.onAttached(attached)
  client.onConnectionChange(status)
  client.start()
  mock.callbacks.opened()
  const [first, second] = mock.send.mock.calls.map(([message]) => message)
  mock.callbacks.message({ id: first.id, type: 'next', payload: { extensions: { subscriptionReady: true } } })
  mock.callbacks.message({ id: second.id, type: 'next', payload: { extensions: { subscriptionReady: true } } })
  mock.callbacks.message({ id: second.id, type: 'error', payload: [{ message: 'resync_required' }] })
  expect(mock.close).not.toHaveBeenCalled()
  const replacement = mock.send.mock.calls.at(-1)![0]
  expect(replacement.type).toBe('subscribe')
  expect(replacement.id).not.toBe(second.id)
  expect(replacement.payload.query).toBe(second.payload.query)
  mock.callbacks.message({ id: second.id, type: 'next', payload: { data: { mediaLibraryUpdated: { types: [1] } } } })
  expect(media).not.toHaveBeenCalled()
  mock.callbacks.message({ id: first.id, type: 'next', payload: { data: { contentChanged: true } } })
  expect(content).toHaveBeenCalledWith(true)
  mock.callbacks.message({ id: replacement.id, type: 'next', payload: { extensions: { subscriptionReady: true } } })
  expect(attached).toHaveBeenCalledTimes(2)
  stopMedia()
  expect(mock.send.mock.calls.at(-1)![0]).toEqual({ id: replacement.id, type: 'complete' })
  expect(status.mock.calls).toEqual([[true]])
  client.stop()
})
