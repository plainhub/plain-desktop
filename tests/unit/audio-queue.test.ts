import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { gqlFetch } from '@/lib/api/gql-client'
import { audioPlayback, resetAudioPlaylistForTests, useAudioPlaylistStore } from '@/hooks/audio-playlist-store'

vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn(), getGraphqlContextKey: () => connectionContext }))
vi.mock('@/lib/api/query', () => ({ audioQueueGQL: 'audio queue' }))

let connectionContext = 'device-a'
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
  connectionContext = 'device-a'
  resetAudioPlaylistForTests()
  fetch.mockReset()
  selectionResults.length = 0
  reportResults.length = 0
  commands.length = 0
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
    fetch.mockResolvedValueOnce({ data: { total: 2, items: tracks(0, 2), playback: { mediaId: '1', path: '/audio/1', mode: 'REPEAT', isPlaying: true, positionMs: 123, revision: 1 } } })
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
  recordMediaPlayGQL: 'record-media-play',
  saveMediaPlayProgressGQL: 'save-media-progress',
  playAudioGQL: 'play',
  enqueueAndPlayAudioGQL: 'enqueue-play',
  advanceAudioSelectionGQL: 'advance',
  reportAudioPlaybackGQL: 'report',
  updateAudioPlayModeGQL: 'mode',
  removeAudioFromQueueGQL: 'remove',
  clearAudioQueueGQL: 'clear',
  reorderAudioQueueGQL: 'reorder',
  initMutation: ({ document }: { document: string }) => {
    const callbacks: Array<(result: any) => void> = []
    return {
      mutate: vi.fn(async (variables: unknown) => {
        commands.push({ document, variables })
        const result = document === 'advance' ? await selectionResults.shift() : document === 'record-media-play'
          ? (reportResults.length ? await reportResults.shift() : { data: { reportAudioPlayback: true } })
          : { data: { updateAudioPlayMode: true } }
        for (const callback of callbacks) callback(result)
        return result
      }),
      loading: false,
      onDone: (callback: (result: any) => void) => { callbacks.push(callback) },
    }
  },
}))
const selectionResults: any[] = []
const reportResults: any[] = []
afterEach(() => { vi.useRealTimers() })
const commands: Array<{ document: string; variables: unknown }> = []

async function mountPlayer(element?: HTMLAudioElement) {
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
  let player!: ReturnType<typeof useAudioPlaylist>
  const app = renderer.createApp({
    setup() {
      player = useAudioPlaylist(ref(element))
      return () => null
    },
  })
  app.mount({})
  return { app, player }
}

it('delegates next and previous across unloaded pages without fetching them', async () => {
  fetch.mockImplementation(async (_document, variables: any) => ({
    data: { total: 450, items: tracks(variables.offset, Math.min(variables.limit, 450 - variables.offset)) },
  }))
  await useAudioPlaylistStore().ensureLoaded()
  const { app, player } = await mountPlayer()
  try {
    commands.length = 0
    audioPlayback.value = { mediaId: '199', path: '/audio/199', mode: 'REPEAT', isPlaying: false, positionMs: 0, revision: 5 }
    const pageRequests = fetch.mock.calls.length
    selectionResults.push({ data: { advanceAudioSelection: { track: tracks(200, 1)[0], path: '/audio/200', revision: 6, positionMs: 0 } } })
    await player.playNext()
    expect(commands.at(-1)).toEqual({ document: 'advance', variables: { action: 'NEXT', expectedPath: '/audio/199', expectedRevision: 5 } })
    expect(player.current.value?.mediaId).toBe('200')
    expect(fetch).toHaveBeenCalledTimes(pageRequests)
    audioPlayback.value = { ...audioPlayback.value, mediaId: '0', path: '/audio/0', revision: 7 }
    selectionResults.push({ data: { advanceAudioSelection: { track: tracks(449, 1)[0], path: '/audio/449', revision: 8, positionMs: 0 } } })
    await player.playPrev()
    expect(commands.at(-1)).toEqual({ document: 'advance', variables: { action: 'PREVIOUS', expectedPath: '/audio/0', expectedRevision: 7 } })
    expect(player.current.value?.mediaId).toBe('449')
    expect(fetch).toHaveBeenCalledTimes(pageRequests)
    await player.changeMode()
    expect(commands.at(-1)).toEqual({ document: 'mode', variables: { mode: 'REPEAT_ONE' } })
    expect(audioPlayback.value.mode).toBe('REPEAT_ONE')
  } finally {
    app.unmount()
  }
})

function mediaElement() {
  return {
    src: '', currentSrc: '', currentTime: 0, ended: true,
    play: vi.fn(async () => {}), pause: vi.fn(),
    addEventListener: vi.fn(), removeEventListener: vi.fn(),
  }
}

it('pins ended callbacks to the played revision and ignores replaced or disposed bindings', async () => {
  fetch.mockResolvedValue({ data: { total: 0, items: [] } })
  const element = mediaElement()
  const { app, player } = await mountPlayer(element as unknown as HTMLAudioElement)
  try {
    selectionResults.push({ data: { advanceAudioSelection: { track: tracks(1,1)[0], path:'/audio/1', revision:1, positionMs:0 } } })
    await player.playNext()
    const firstEnded = element.addEventListener.mock.calls.filter(([type]) => type === 'ended').at(-1)![1]
    selectionResults.push({ data: { advanceAudioSelection: { track: tracks(2,1)[0], path:'/audio/2', revision:2, positionMs:0 } } })
    await player.playNext()
    await firstEnded()
    expect(commands).toHaveLength(2)
    const currentEnded = element.addEventListener.mock.calls.filter(([type]) => type === 'ended').at(-1)![1]
    selectionResults.push({ data: { advanceAudioSelection: { track: tracks(2,1)[0], path:'/audio/2', revision:3, positionMs:0 } } })
    await currentEnded()
    expect(commands.at(-1)).toEqual({ document:'advance',variables:{action:'COMPLETED',expectedPath:'/audio/2',expectedRevision:2} })
    expect(audioPlayback.value.revision).toBe(3)
    expect(element.currentTime).toBe(0)
    app.unmount()
    const count = commands.length
    await currentEnded()
    expect(commands).toHaveLength(count)
  } finally {
    if (app._instance) app.unmount()
  }
})

it('does not start browser playback from a response that arrives after unmount', async () => {
  fetch.mockResolvedValue({ data: { total: 0, items: [] } })
  let finish!: (value: unknown) => void
  selectionResults.push(new Promise((resolve) => { finish = resolve }))
  const element = mediaElement()
  const { app, player } = await mountPlayer(element as unknown as HTMLAudioElement)
  const request = player.playNext()
  await Promise.resolve()
  app.unmount()
  finish({ data: { advanceAudioSelection: { track:tracks(1,1)[0],path:'/audio/1',revision:1,positionMs:0 } } })
  await request
  expect(element.play).not.toHaveBeenCalled()
  expect(audioPlayback.value.path).toBeNull()
})


it('discards old-device responses and commands queued before the device changed', async () => {
  fetch.mockResolvedValue({ data: { total: 0, items: [] } })
  let finish!: (value: unknown) => void
  selectionResults.push(new Promise((resolve) => { finish = resolve }))
  const element = mediaElement()
  const { app, player } = await mountPlayer(element as unknown as HTMLAudioElement)
  try {
    const first = player.playNext()
    await Promise.resolve()
    const queued = player.playPrev()
    connectionContext = 'device-b'
    finish({ data: { advanceAudioSelection: { track: tracks(1,1)[0], path:'/audio/1', revision:1, positionMs:0 } } })
    await Promise.all([first, queued])
    expect(commands).toHaveLength(1)
    expect(element.play).not.toHaveBeenCalled()
    expect(audioPlayback.value.path).toBeNull()
  } finally { app.unmount() }
})

it('ignores an old-device ended callback after switching the connection', async () => {
  fetch.mockResolvedValue({ data: { total: 0, items: [] } })
  const element = mediaElement()
  const { app, player } = await mountPlayer(element as unknown as HTMLAudioElement)
  try {
    selectionResults.push({ data: { advanceAudioSelection: { track: tracks(1,1)[0], path:'/audio/1', revision:1, positionMs:0 } } })
    await player.playNext()
    const ended = element.addEventListener.mock.calls.filter(([type]) => type === 'ended').at(-1)![1]
    connectionContext = 'device-b'
    await ended()
    expect(commands).toHaveLength(1)
  } finally { app.unmount() }
})


async function startBrowserPlayer() {
  fetch.mockResolvedValue({ data: { total: 0, items: [] } })
  const element = mediaElement()
  const mounted = await mountPlayer(element as unknown as HTMLAudioElement)
  selectionResults.push({ data: { advanceAudioSelection: { track: tracks(1,1)[0], path:'/audio/1', revision:7, positionMs:0 } } })
  await mounted.player.playNext()
  const playing = element.addEventListener.mock.calls.filter(([type]) => type === 'playing').at(-1)![1]
  const pause = element.addEventListener.mock.calls.filter(([type]) => type === 'pause').at(-1)![1]
  return { ...mounted, element, playing, pause }
}

it('reports actual browser playing rather than play intent and samples progress in milliseconds', async () => {
  vi.useFakeTimers()
  const { app, element, playing } = await startBrowserPlayer()
  try {
    expect(commands.filter((command) => command.document === 'record-media-play')).toHaveLength(0)
    element.currentTime = 0.123
    playing()
    await vi.advanceTimersByTimeAsync(0)
    expect(commands.at(-1)).toEqual({ document:'record-media-play', variables:{ mediaId:'1', type:'AUDIO', positionMs:123 } })
    element.currentTime = 2.45
    await vi.advanceTimersByTimeAsync(1000)
    expect(commands.at(-1)).toEqual({ document:'save-media-progress', variables:{ mediaId:'1', type:'AUDIO', positionMs:2450 } })
  } finally { app.unmount() }
  const count = commands.length
  await vi.advanceTimersByTimeAsync(5000)
  playing()
  expect(commands).toHaveLength(count)
})

it('serializes playback history and records the pause position', async () => {
  vi.useFakeTimers()
  const { app, element, playing, pause } = await startBrowserPlayer()
  try {
    let finish!: (value: unknown) => void
    reportResults.push(new Promise((resolve) => { finish = resolve }))
    playing()
    await vi.advanceTimersByTimeAsync(0)
    element.currentTime = 3
    pause()
    expect(commands.filter((command) => command.document === 'save-media-progress')).toHaveLength(0)
    finish({ data: { recordMediaPlay: true } })
    await vi.advanceTimersByTimeAsync(0)
    expect(commands.at(-1)).toEqual({ document:'save-media-progress', variables:{ mediaId:'1', type:'AUDIO', positionMs:3000 } })
  } finally { app.unmount() }
})

it('does not send queued history to a different device', async () => {
  vi.useFakeTimers()
  const { app, element, playing, pause } = await startBrowserPlayer()
  try {
    let finish!: (value: unknown) => void
    reportResults.push(new Promise((resolve) => { finish = resolve }))
    playing()
    await vi.advanceTimersByTimeAsync(0)
    element.currentTime = 3
    pause()
    connectionContext = 'device-b'
    finish({ data: { recordMediaPlay: true } })
    await vi.advanceTimersByTimeAsync(0)
    expect(commands.filter((command) => command.document === 'save-media-progress')).toHaveLength(0)
  } finally { app.unmount() }
})
