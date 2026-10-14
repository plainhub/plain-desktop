import { beforeEach, describe, expect, it, vi } from 'vitest'
import { gqlFetch } from '@/lib/api/gql-client'
import { audioPlayback, resetAudioPlaylistForTests, useAudioPlaylistStore } from '@/hooks/audio-playlist-store'

vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn() }))
vi.mock('@/lib/api/query', () => ({ audioQueueGQL: 'audio queue' }))

const fetch = vi.mocked(gqlFetch)
const tracks = (offset: number, count: number) =>
  Array.from({ length: count }, (_, i) => ({
    mediaId: String(offset + i),
    path: `/audio/${offset + i}`,
    title: 'Track',
    artist: 'Artist',
    durationMs: 123,
  }))

beforeEach(() => {
  resetAudioPlaylistForTests()
  fetch.mockReset()
})

describe('audio queue pages', () => {
  it('loads beyond a page and refetches the complete loaded prefix', async () => {
    const store = useAudioPlaylistStore()
    fetch.mockResolvedValueOnce({ data: { total: 1205, items: tracks(0, 200) } })
    await store.ensureLoaded()
    fetch.mockResolvedValueOnce({ data: { total: 1205, items: tracks(200, 200) } })
    await store.loadMore()
    expect(store.items.value).toHaveLength(400)
    expect(store.items.value[399]?.mediaId).toBe('399')
    fetch.mockResolvedValueOnce({ data: { total: 1205, items: tracks(0, 400) } })
    await store.refetch()
    expect(fetch).toHaveBeenLastCalledWith('audio queue', { offset: 0, limit: 400 })
    expect(store.items.value).toHaveLength(400)
    expect(store.total.value).toBe(1205)
  })

  it('merges pages by media ID even if the file path changed', async () => {
    const store = useAudioPlaylistStore()
    fetch.mockResolvedValueOnce({ data: { total: 201, items: tracks(0, 200) } })
    await store.ensureLoaded()
    fetch.mockResolvedValueOnce({ data: { total: 201, items: [{ ...tracks(199, 1)[0]!, path: '/moved' }, ...tracks(200, 1)] } })
    await store.loadMore()
    expect(store.items.value).toHaveLength(201)
    expect(store.items.value.map((item) => item.mediaId)).toEqual(tracks(0, 201).map((item) => item.mediaId))
  })

  it('keeps the last successful view when fetching fails', async () => {
    const store = useAudioPlaylistStore()
    fetch.mockResolvedValueOnce({ data: { total: 2, items: tracks(0, 2), playback: { mediaId: '1', path: '/audio/1', mode: 'REPEAT', isPlaying: true, positionMs: 123 } } })
    await store.ensureLoaded()
    fetch.mockRejectedValueOnce(new Error('offline'))
    await store.refetch()
    expect(store.items.value).toEqual(tracks(0, 2))
    expect(audioPlayback.value.mediaId).toBe('1')
    expect(store.loading.value).toBe(false)
  })

  it('ignores a stale page that finishes after a newer refresh', async () => {
    const store = useAudioPlaylistStore()
    let resolveOld!: (value: any) => void
    fetch.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOld = resolve
        })
    )
    const old = store.fetchInitial()
    fetch.mockResolvedValueOnce({ data: { total: 1, items: tracks(10, 1) } })
    await store.refetch()
    resolveOld({ data: { total: 2, items: tracks(0, 2) } })
    await old
    expect(store.items.value).toEqual(tracks(10, 1))
    expect(store.total.value).toBe(1)
  })
})

vi.mock('@/stores/temp', async () => {
  const { ref } = await import('vue')
  return { useTempStore: () => ({ app: ref({}), urlTokenKey: ref(null), audioPlaying: ref(false) }) }
})
vi.mock('@/lib/api/file', () => ({ getFileUrlByPath: (_: unknown, path: string) => path }))
vi.mock('@/lib/api/mutation', () => ({
  playAudioGQL: 'play',
  enqueueAndPlayAudioGQL: 'enqueue-play',
  updateAudioPlayModeGQL: 'mode',
  removeAudioFromQueueGQL: 'remove',
  clearAudioQueueGQL: 'clear',
  reorderAudioQueueGQL: 'reorder',
  initMutation: ({ document }: { document: string }) => ({
    mutate: vi.fn(async (variables: unknown) => {
      commands.push({ document, variables })
    }),
    loading: false,
    onDone: vi.fn(),
  }),
}))
const commands: Array<{ document: string; variables: unknown }> = []

it('next and previous navigate across the complete paged queue', async () => {
  const { createRenderer, ref } = await import('vue')
  const { useAudioPlaylist } = await import('@/hooks/audio-player')
  const renderer = createRenderer<any, any>({
    insert: vi.fn(),
    remove: vi.fn(),
    createElement: () => ({}),
    createText: () => ({}),
    createComment: () => ({}),
    setText: vi.fn(),
    setElementText: vi.fn(),
    parentNode: () => null,
    nextSibling: () => null,
    patchProp: vi.fn(),
  })
  fetch.mockImplementation(async (_document, variables: any) => ({
    data: { total: 450, items: tracks(variables.offset, Math.min(variables.limit, 450 - variables.offset)) },
  }))
  await useAudioPlaylistStore().ensureLoaded()
  let player!: ReturnType<typeof useAudioPlaylist>
  const app = renderer.createApp({
    setup() {
      player = useAudioPlaylist(ref(undefined))
      return () => null
    },
  })
  app.mount({})
  try {
    commands.length = 0
    audioPlayback.value = { mediaId: '199', path: '/audio/199', mode: 'REPEAT', isPlaying: false, positionMs: 0 }
    await player.playNext()
    expect(commands.at(-1)).toEqual({ document: 'play', variables: { mediaId: '200' } })
    audioPlayback.value = { ...audioPlayback.value, mediaId: '0', path: '/audio/0' }
    await player.playPrev()
    expect(commands.at(-1)).toEqual({ document: 'play', variables: { mediaId: '449' } })
  } finally {
    app.unmount()
  }
})
