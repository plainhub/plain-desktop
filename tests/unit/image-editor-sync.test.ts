import { afterEach, expect, it, vi } from 'vitest'
import * as Y from 'yjs'

const mock = vi.hoisted(() => ({ listeners: new Map<string, Set<(value: any) => void>>(), dropChunk: false, failures: 0 }))
vi.mock('@/lib/api/api', () => ({ isNativeTarget: () => true }))
vi.mock('@/plugins/eventbus', () => ({
  default: {
    on(name: string, handler: (value: any) => void) {
      const listeners = mock.listeners.get(name) ?? new Set()
      listeners.add(handler)
      mock.listeners.set(name, listeners)
    },
    off(name: string, handler: (value: any) => void) { mock.listeners.get(name)?.delete(handler) },
    emit(name: string, value: any) { mock.listeners.get(name)?.forEach(handler => handler(value)) },
  },
}))
vi.mock('@/lib/api/gql-client', () => ({
  async gqlFetch(_query: string, variables: { id: string; update: string }) {
    if (mock.failures > 0) { mock.failures--; throw new Error('connection lost') }
    const update = Uint8Array.from(Buffer.from(variables.update, 'base64'))
    if (mock.dropChunk && update[4] === 1 && new DataView(update.buffer).getUint16(21) === 1) return { data: {} }
    const id = new TextEncoder().encode(variables.id)
    const frame = new Uint8Array(1 + id.length + update.length)
    frame[0] = id.length
    frame.set(id, 1)
    frame.set(update, 1 + id.length)
    mock.listeners.get('IMAGE_EDITOR_UPDATE')?.forEach(handler => handler(frame))
    return { data: {} }
  },
}))
vi.mock('@/views/image-editor/sync/image-editor-socket', () => ({ ImageEditorSocket: class {} }))

import { EventSyncTransport } from '../../src/views/image-editor/sync/event-sync-transport'

afterEach(() => {
  vi.useRealTimers()
  mock.dropChunk = false
  mock.failures = 0
  mock.listeners.clear()
})

it('retries state exchange when a mutation fails', async () => {
  vi.useFakeTimers()
  vi.spyOn(console, 'error').mockImplementation(() => {})
  const source = new Y.Doc()
  const destination = new Y.Doc()
  source.getMap('content').set('value', 'saved')
  const a = new EventSyncTransport(() => 'project', () => Y.encodeStateAsUpdate(source))
  const b = new EventSyncTransport(() => 'project', () => Y.encodeStateAsUpdate(destination))
  b.onUpdate(update => Y.applyUpdate(destination, update))
  await b.connect()
  await vi.advanceTimersByTimeAsync(0)
  mock.failures = 2
  await a.connect()
  await vi.advanceTimersByTimeAsync(0)
  expect(destination.getMap('content').get('value')).toBeUndefined()
  await vi.advanceTimersByTimeAsync(1000)
  expect(destination.getMap('content').get('value')).toBe('saved')
  a.destroy()
  b.destroy()
  vi.restoreAllMocks()
})

it('reassembles large updates and resynchronizes both documents after disconnection', async () => {
  const first = new Y.Doc()
  const second = new Y.Doc()
  const a = new EventSyncTransport(() => 'project', () => Y.encodeStateAsUpdate(first))
  const b = new EventSyncTransport(() => 'project', () => Y.encodeStateAsUpdate(second))
  a.onUpdate(update => Y.applyUpdate(first, update, 'remote'))
  b.onUpdate(update => Y.applyUpdate(second, update, 'remote'))
  first.on('update', (update, origin) => { if (origin !== 'remote') a.broadcastUpdate(update) })
  second.on('update', (update, origin) => { if (origin !== 'remote') b.broadcastUpdate(update) })
  await a.connect()
  await b.connect()
  first.getMap('content').set('large', 'x'.repeat(600_000))
  await vi.waitFor(() => expect(second.getMap('content').get('large')).toBe('x'.repeat(600_000)))
  a.disconnect()
  first.getMap('content').set('offline', 'first')
  second.getMap('content').set('online', 'second')
  await a.connect()
  await vi.waitFor(() => {
    expect(first.getMap('content').get('online')).toBe('second')
    expect(second.getMap('content').get('offline')).toBe('first')
  })
  a.destroy()
  b.destroy()
})

it('does not deliver an incomplete fragmented update', async () => {
  vi.useFakeTimers()
  const source = new Y.Doc()
  const destination = new Y.Doc()
  const received = vi.fn((update: Uint8Array) => Y.applyUpdate(destination, update))
  const a = new EventSyncTransport(() => 'project', () => Y.encodeStateAsUpdate(source))
  const b = new EventSyncTransport(() => 'project', () => Y.encodeStateAsUpdate(destination))
  b.onUpdate(received)
  await a.connect()
  await b.connect()
  await vi.advanceTimersByTimeAsync(0)
  expect(received).toHaveBeenCalled()
  received.mockClear()
  mock.dropChunk = true
  source.getMap('content').set('large', 'x'.repeat(600_000))
  a.broadcastUpdate(Y.encodeStateAsUpdate(source))
  await vi.advanceTimersByTimeAsync(200)
  expect(received).not.toHaveBeenCalled()
  mock.dropChunk = false
  await vi.advanceTimersByTimeAsync(30_000)
  expect(destination.getMap('content').get('large')).toBe('x'.repeat(600_000))
  a.destroy()
  b.destroy()
})
