const subscriptionMock = vi.hoisted(() => ({ listeners: new Map<string, Set<(value: any) => void>>() }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (field: string, callback: (value: any) => void) => {
    const listeners = subscriptionMock.listeners.get(field) ?? new Set()
    subscriptionMock.listeners.set(field, listeners)
    listeners.add(callback)
    return () => listeners.delete(callback)
  },
} }))
function publish(field: string, value: unknown) {
  for (const callback of subscriptionMock.listeners.get(field) ?? []) callback(value)
}
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { DataType } from '../../src/lib/data'
import { subscribeMediaLibraryUpdates } from '../../src/lib/media/library-updates'

const stops: (() => void)[] = []
beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(0) })
afterEach(() => { for (const stop of stops.splice(0)) stop(); vi.useRealTimers() })

it('refreshes only matching media consumers and removes their subscriptions', () => {
  const image = vi.fn()
  const audio = vi.fn()
  const stopImage = subscribeMediaLibraryUpdates(DataType.IMAGE, image)
  stops.push(stopImage, subscribeMediaLibraryUpdates(DataType.AUDIO, audio))
  publish('mediaLibraryUpdated', { types: [3] })
  vi.advanceTimersByTime(2000)
  expect(image).toHaveBeenCalledTimes(1)
  expect(audio).not.toHaveBeenCalled()
  publish('mediaLibraryUpdated', { types: [1, 3, 3] })
  vi.advanceTimersByTime(2000)
  expect(image).toHaveBeenCalledTimes(2)
  expect(audio).toHaveBeenCalledTimes(1)
  stopImage()
  publish('mediaLibraryUpdated', { types: [3] })
  vi.advanceTimersByTime(2000)
  expect(image).toHaveBeenCalledTimes(2)
})

it('does not translate unsupported data domains into a media refresh', () => {
  const refresh = vi.fn()
  stops.push(subscribeMediaLibraryUpdates(DataType.NOTE, refresh))
  publish('mediaLibraryUpdated', { types: [1, 2, 3, 24] })
  expect(refresh).not.toHaveBeenCalled()
})

it.each([[DataType.VIDEO, 2], [DataType.DOC, 24]] as const)('refreshes %s from its shared wire kind', (type, kind) => {
  const refresh = vi.fn()
  stops.push(subscribeMediaLibraryUpdates(type, refresh))
  publish('mediaLibraryUpdated', { types: [kind] })
  vi.advanceTimersByTime(2000)
  expect(refresh).toHaveBeenCalledTimes(1)
})

it('coalesces scan bursts, refreshes during sustained indexing and cancels pending work on leave', () => {
  const refresh = vi.fn()
  const stop = subscribeMediaLibraryUpdates(DataType.AUDIO, refresh)
  stops.push(stop)
  for (let second = 0; second < 10; second++) {
    for (let file = 0; file < 100; file++) publish('mediaLibraryUpdated', { types: [1] })
    vi.advanceTimersByTime(1000)
  }
  expect(refresh).toHaveBeenCalledTimes(1)
  publish('mediaLibraryUpdated', { types: [1] })
  vi.advanceTimersByTime(2000)
  expect(refresh).toHaveBeenCalledTimes(2)
  publish('mediaLibraryUpdated', { types: [1] })
  stop()
  vi.advanceTimersByTime(10000)
  expect(refresh).toHaveBeenCalledTimes(2)
})
