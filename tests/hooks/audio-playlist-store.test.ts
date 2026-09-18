import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

const gqlFetchMock = vi.fn()
vi.mock('@/lib/api/gql-client', () => {
  class GqlError extends Error {}
  return { gqlFetch: (...args: any[]) => gqlFetchMock(...args), GqlError }
})

vi.mock('@/lib/api/query', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api/query')>()),
  audioQueueGQL: 'audioQueue-query',
}))

import { resetAudioPlaylistForTests, useAudioPlaylistStore } from '@/hooks/audio-playlist-store'

function playlistResult(items: Array<{ path: string }>, total = items.length) {
  return { data: { items, total } }
}

beforeEach(() => {
  setActivePinia(createPinia())
  resetAudioPlaylistForTests()
  gqlFetchMock.mockReset()
})

describe('useAudioPlaylistStore', () => {
  it('fetches once per session via ensureLoaded', async () => {
    gqlFetchMock.mockResolvedValue(playlistResult([{ path: '/a.mp3' }]))

    const store = useAudioPlaylistStore()
    await store.ensureLoaded()
    await store.ensureLoaded()

    expect(gqlFetchMock).toHaveBeenCalledTimes(1)
    expect(store.items.value.map((it) => it.path)).toEqual(['/a.mp3'])
    expect(store.total.value).toBe(1)
  })

  it('retries the initial fetch when it fails', async () => {
    gqlFetchMock.mockRejectedValueOnce(new Error('network down'))
    gqlFetchMock.mockResolvedValueOnce(playlistResult([{ path: '/a.mp3' }]))

    const store = useAudioPlaylistStore()
    await store.ensureLoaded()
    expect(store.items.value).toEqual([])

    await store.ensureLoaded()
    expect(store.items.value.map((it) => it.path)).toEqual(['/a.mp3'])
    expect(gqlFetchMock).toHaveBeenCalledTimes(2)
  })

  it('does not treat a GraphQL error response as loaded', async () => {
    gqlFetchMock.mockResolvedValue({ errors: [{ message: 'no permission' }] })

    const store = useAudioPlaylistStore()
    await store.ensureLoaded()
    expect(store.items.value).toEqual([])

    gqlFetchMock.mockResolvedValue(playlistResult([{ path: '/a.mp3' }]))
    await store.ensureLoaded()
    expect(gqlFetchMock).toHaveBeenCalledTimes(2)
  })

  it('refetch replaces the mirrored queue with server truth', async () => {
    gqlFetchMock.mockResolvedValueOnce(playlistResult([{ path: '/a.mp3' }, { path: '/b.mp3' }]))
    gqlFetchMock.mockResolvedValueOnce(playlistResult([{ path: '/b.mp3' }]))

    const store = useAudioPlaylistStore()
    await store.fetchInitial()
    expect(store.items.value).toHaveLength(2)

    await store.refetch()
    expect(store.items.value.map((it) => it.path)).toEqual(['/b.mp3'])
    expect(store.total.value).toBe(1)
  })

  it('removeLocal drops the item from the mirror without a request', async () => {
    gqlFetchMock.mockResolvedValue(playlistResult([{ path: '/a.mp3' }, { path: '/b.mp3' }]))

    const store = useAudioPlaylistStore()
    await store.fetchInitial()

    store.removeLocal('/a.mp3')

    expect(gqlFetchMock).toHaveBeenCalledTimes(1)
    expect(store.items.value.map((it) => it.path)).toEqual(['/b.mp3'])
    expect(store.total.value).toBe(1)
  })
})
