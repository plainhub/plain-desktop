import { afterEach, expect, it, vi } from 'vitest'
import { DataType } from '../../src/lib/data'
import emitter from '../../src/plugins/eventbus'
import { subscribeMediaLibraryUpdates } from '../../src/lib/media/library-updates'

const stops: (() => void)[] = []
afterEach(() => { for (const stop of stops.splice(0)) stop() })

it('refreshes only matching media consumers and removes their subscriptions', () => {
  const image = vi.fn()
  const audio = vi.fn()
  const stopImage = subscribeMediaLibraryUpdates(DataType.IMAGE, image)
  stops.push(stopImage, subscribeMediaLibraryUpdates(DataType.AUDIO, audio))
  emitter.emit('MEDIA_LIBRARY_UPDATED', { types: [3] })
  expect(image).toHaveBeenCalledTimes(1)
  expect(audio).not.toHaveBeenCalled()
  emitter.emit('MEDIA_LIBRARY_UPDATED', { types: [1, 3, 3] })
  expect(image).toHaveBeenCalledTimes(2)
  expect(audio).toHaveBeenCalledTimes(1)
  stopImage()
  emitter.emit('MEDIA_LIBRARY_UPDATED', { types: [3] })
  expect(image).toHaveBeenCalledTimes(2)
})

it.each([null, {}, { types: [] }, { types: ['3'] }, { types: [3, 99] }, { types: [3, 3, 3, 3, 3] }])(
  'ignores malformed or unsupported library changes: %j', payload => {
    const refresh = vi.fn()
    stops.push(subscribeMediaLibraryUpdates(DataType.IMAGE, refresh))
    emitter.emit('MEDIA_LIBRARY_UPDATED', payload as { types: number[] })
    expect(refresh).not.toHaveBeenCalled()
  },
)

it('does not translate unsupported data domains into a media refresh', () => {
  const refresh = vi.fn()
  stops.push(subscribeMediaLibraryUpdates(DataType.NOTE, refresh))
  emitter.emit('MEDIA_LIBRARY_UPDATED', { types: [1, 2, 3, 24] })
  expect(refresh).not.toHaveBeenCalled()
})

it.each([[DataType.VIDEO, 2], [DataType.DOC, 24]] as const)('refreshes %s from its shared wire kind', (type, kind) => {
  const refresh = vi.fn()
  stops.push(subscribeMediaLibraryUpdates(type, refresh))
  emitter.emit('MEDIA_LIBRARY_UPDATED', { types: [kind] })
  expect(refresh).toHaveBeenCalledTimes(1)
})
