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
