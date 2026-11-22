import { expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ callbacks: null as any, close: vi.fn() }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/api/subscription-connection', () => ({ openSubscriptionConnection: (callbacks: unknown) => {
  mock.callbacks = callbacks
  return { send: vi.fn(), close: mock.close }
} }))
import { SubscriptionClient } from '../../src/lib/api/subscription-client'

it('reports authenticated GraphQL connection loss, recovery and stop', () => {
  vi.useFakeTimers()
  vi.spyOn(console, 'error').mockImplementation(() => {})
  const client = new SubscriptionClient()
  const status = vi.fn()
  const stopListening = client.onConnectionChange(status)
  client.start()
  mock.callbacks.opened()
  mock.callbacks.dropped(new Error('disconnected'))
  vi.advanceTimersByTime(1000)
  mock.callbacks.opened()
  client.stop()
  expect(status.mock.calls).toEqual([[true], [false], [true], [null]])
  stopListening()
  vi.restoreAllMocks()
  vi.useRealTimers()
})

it('refreshes the view once when Core restores subscriptions on the same handle', () => {
  const sent: { id: string }[] = []
  let callbacks!: NonNullable<typeof mock.callbacks>
  const client = new SubscriptionClient(value => {
    callbacks = value
    return { send: message => sent.push(message as { id: string }), close: vi.fn() }
  })
  const refresh = vi.fn()
  client.onAttached(refresh)
  client.subscribe('contentChanged', vi.fn())
  client.subscribe('mediaUpdated', vi.fn())
  client.setOperations([
    { field: 'contentChanged', document: 'subscription { contentChanged }' },
    { field: 'mediaUpdated', document: 'subscription { mediaUpdated { types } }' },
  ])
  client.start()
  callbacks.opened()
  const ready = (index: number) => callbacks.message({
    id: sent[index].id, type: 'next', payload: { extensions: { subscriptionReady: true } },
  })
  ready(0)
  ready(1)
  expect(refresh).toHaveBeenCalledTimes(1)
  ready(1)
  expect(refresh).toHaveBeenCalledTimes(1)
  ready(0)
  expect(refresh).toHaveBeenCalledTimes(2)
  expect(sent).toHaveLength(2)
  client.stop()
})
