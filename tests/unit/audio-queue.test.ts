import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { gqlFetch } from '@/lib/api/gql-client'
import { audioPlayMode, audioPlayback, resetAudioPlaylistForTests, useAudioPlaylistStore } from '@/hooks/audio-playlist-store'

vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn(), getGraphqlContextKey: () => connectionContext }))
vi.mock('@/lib/api/query', () => ({ audioQueueGQL: 'audio queue', mediaPlayHistoryGQL: 'media history' }))

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
    fetch.mockResolvedValueOnce({ data: { total: 2, items: tracks(0, 2), playback: { mediaId: '1', path: '/audio/1' }, prefs: { audio_play_mode: 'SHUFFLE' } } })
    await store.ensureLoaded()
    fetch.mockRejectedValueOnce(new Error('offline'))
    await store.refetch()
    expect(store.items.value).toEqual(tracks(0, 2))
    expect(audioPlayback.value.mediaId).toBe('1')
    expect(audioPlayMode.value).toBe('SHUFFLE')
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
  selectAudioGQL: 'play',
  enqueueAndSelectAudioGQL: 'enqueue-play',
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
        const result = document === 'record-media-play'
          ? (reportResults.length ? await reportResults.shift() : { data: { reportAudioPlayback: true } })
          : document === 'play' ? (selectionResults.length ? await selectionResults.shift() : { data: { selectAudio: tracks(Number((variables as { mediaId: string }).mediaId), 1)[0] } })
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

it('selects across queue pages and wraps backwards without a dedicated navigation API', async () => {
  fetch.mockImplementation(async (_document, variables: any) => ({
    data: { total: 450, items: tracks(variables.offset, Math.min(variables.limit, 450 - variables.offset)) },
  }))
  await useAudioPlaylistStore().ensureLoaded()
  const { app, player } = await mountPlayer()
  try {
    audioPlayback.value = { mediaId: '199', path: '/audio/199' }
    await player.playNext()
    expect(commands.at(-1)).toEqual({ document: 'play', variables: { mediaId: '200' } })
    expect(player.current.value?.mediaId).toBe('200')
    expect(fetch).toHaveBeenCalledWith('audio queue', { offset: 200, limit: 200 })
    audioPlayback.value = { ...audioPlayback.value, mediaId: '0', path: '/audio/0' }
    await player.playPrev()
    expect(commands.at(-1)).toEqual({ document: 'play', variables: { mediaId: '449' } })
    expect(player.current.value?.mediaId).toBe('449')
    expect(fetch).toHaveBeenCalledWith('audio queue', { offset: 400, limit: 200 })
    await player.changeMode()
    expect(commands.at(-1)).toEqual({ document: 'mode', variables: { mode: 'REPEAT_ONE' } })
    expect(audioPlayMode.value).toBe('REPEAT_ONE')
  } finally {
    app.unmount()
  }
})

it('can shuffle into an unloaded page without choosing the current track', async () => {
  fetch.mockImplementation(async (_document, variables: any) => ({
    data: { total: 300, items: tracks(variables.offset, Math.min(variables.limit, 300 - variables.offset)) },
  }))
  const random = vi.spyOn(Math, 'random').mockReturnValue(0.9)
  await useAudioPlaylistStore().ensureLoaded()
  const { app, player } = await mountPlayer()
  try {
    audioPlayback.value = { mediaId: '269', path: '/audio/269' }
    audioPlayMode.value = 'SHUFFLE'
    await player.playNext()
    expect(commands).toEqual([{ document: 'play', variables: { mediaId: '270' } }])
    expect(fetch).toHaveBeenCalledWith('audio queue', { offset: 200, limit: 200 })
  } finally {
    random.mockRestore()
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

it('discards replaced callbacks and repeats the active track without a selection request', async () => {
  fetch.mockResolvedValue({ data: { total: 2, items: tracks(1, 2) } })
  await useAudioPlaylistStore().ensureLoaded()
  const element = mediaElement()
  const { app, player } = await mountPlayer(element as unknown as HTMLAudioElement)
  try {
    await player.playNext()
    const firstEnded = element.addEventListener.mock.calls.filter(([type]) => type === 'ended').at(-1)![1]
    await player.playNext()
    expect(element.removeEventListener).toHaveBeenCalledWith('ended', firstEnded)
    await firstEnded()
    expect(commands).toHaveLength(2)
    audioPlayMode.value = 'REPEAT_ONE'
    const currentEnded = element.addEventListener.mock.calls.filter(([type]) => type === 'ended').at(-1)![1]
    element.currentTime = 4
    await currentEnded()
    expect(commands).toHaveLength(2)
    expect(element.play).toHaveBeenCalledTimes(3)
    expect(element.currentTime).toBe(0)
    const repeatedEnded = element.addEventListener.mock.calls.filter(([type]) => type === 'ended').at(-1)![1]
    expect(repeatedEnded).not.toBe(currentEnded)
    await repeatedEnded()
    expect(element.play).toHaveBeenCalledTimes(4)
    expect(commands).toHaveLength(2)
    app.unmount()
    await currentEnded()
    expect(commands).toHaveLength(2)
  } finally {
    if (app._instance) app.unmount()
  }
})

it('keeps an ended listener for the active track when fetching the next page fails', async () => {
  fetch.mockResolvedValue({ data: { total: 201, items: tracks(0, 200) } })
  await useAudioPlaylistStore().ensureLoaded()
  const element = mediaElement()
  const { app, player } = await mountPlayer(element as unknown as HTMLAudioElement)
  try {
    player.playItem(tracks(199, 1)[0]!)
    await vi.waitFor(() => expect(element.play).toHaveBeenCalledOnce())
    const previous = element.addEventListener.mock.calls.filter(([type]) => type === 'ended').at(-1)![1]
    fetch.mockRejectedValueOnce(new Error('offline'))
    await player.playNext()
    expect(element.removeEventListener).toHaveBeenCalledWith('ended', previous)
    const current = element.addEventListener.mock.calls.filter(([type]) => type === 'ended').at(-1)![1]
    expect(current).not.toBe(previous)
    expect(audioPlayback.value.mediaId).toBe('199')
    expect(commands).toHaveLength(1)
  } finally {
    app.unmount()
  }
})

it.each(['unmount', 'connection change'] as const)('does not play a pending selection after %s', async (scenario) => {
  fetch.mockResolvedValue({ data: { total: 2, items: tracks(1, 2) } })
  await useAudioPlaylistStore().ensureLoaded()
  let finish!: (value: unknown) => void
  selectionResults.push(new Promise((resolve) => { finish = resolve }))
  const element = mediaElement()
  const { app, player } = await mountPlayer(element as unknown as HTMLAudioElement)
  try {
    const request = player.playNext()
    await vi.waitFor(() => expect(commands).toHaveLength(1))
    if (scenario === 'unmount') app.unmount()
    else connectionContext = 'device-b'
    finish({ data: { selectAudio: tracks(1, 1)[0] } })
    await request
    expect(element.play).not.toHaveBeenCalled()
    expect(audioPlayback.value.path).toBeNull()
  } finally { if (app._instance) app.unmount() }
})

it('lets an explicit track selection supersede in-flight queue navigation', async () => {
  const { app, player, element } = await startBrowserPlayer()
  try {
    let finish!: (value: unknown) => void
    selectionResults.push(new Promise((resolve) => { finish = resolve }))
    const navigation = player.playNext()
    await vi.waitFor(() => expect(commands).toHaveLength(2))
    player.playItem(tracks(3, 1)[0]!)
    finish({ data: { selectAudio: tracks(2, 1)[0] } })
    await navigation
    await vi.waitFor(() => expect(audioPlayback.value.mediaId).toBe('3'))
    expect(element.play).toHaveBeenCalledTimes(2)
  } finally { app.unmount() }
})

it('ignores an old-device ended callback after switching the connection', async () => {
  const { app, element } = await startBrowserPlayer()
  try {
    const ended = element.addEventListener.mock.calls.filter(([type]) => type === 'ended').at(-1)![1]
    connectionContext = 'device-b'
    await ended()
    expect(commands).toHaveLength(1)
  } finally { app.unmount() }
})

async function startBrowserPlayer() {
  fetch.mockResolvedValue({ data: { total: 3, items: tracks(1, 3) } })
  await useAudioPlaylistStore().ensureLoaded()
  const element = mediaElement()
  const mounted = await mountPlayer(element as unknown as HTMLAudioElement)
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


it.each(['restored', 'reselected', 'played', 'disposed'] as const)('restores the initial browser audio position only while %s selection remains current', async scenario => {
  let finish!: (value: unknown) => void
  const playback = { mediaId: '1', path: '/audio/1' }
  fetch.mockImplementation(async document => document === 'media history'
    ? await new Promise(resolve => { finish = resolve })
    : { data: { total: 1, items: tracks(1, 1), playback } })
  const element = mediaElement()
  const { app, player } = await mountPlayer(element as unknown as HTMLAudioElement)
  try {
    await vi.waitFor(() => expect(audioPlayback.value.path).toBe('/audio/1'))
    const loaded = element.addEventListener.mock.calls.find(([type]) => type === 'loadedmetadata')?.[1]
    expect(loaded).toBeTypeOf('function')
    const restoring = loaded!()
    await vi.waitFor(() => expect(finish).toBeTypeOf('function'))
    if (scenario === 'reselected') {
      player.playItem(tracks(1, 1)[0]!)
      await vi.waitFor(() => expect(element.play).toHaveBeenCalledOnce())
    }
    if (scenario === 'played') element.addEventListener.mock.calls.find(([type]) => type === 'play')![1]()
    if (scenario === 'disposed') app.unmount()
    finish({ data: { mediaPlayHistory: [{ positionMs: 30500 }] } })
    await restoring
    await vi.waitFor(() => expect(element.currentTime).toBe(scenario === 'restored' ? 30.5 : 0))
    const selects = scenario === 'reselected' ? 1 : 0
    expect(element.play).toHaveBeenCalledTimes(selects)
    expect(commands).toHaveLength(selects)
  } finally { if (app._instance) app.unmount() }
})

it('starts an explicitly reselected audio at the authoritative selection position, not its restored browser history', async () => {
  const playback = { mediaId: '1', path: '/audio/1' }
  fetch.mockResolvedValue({ data: { total: 1, items: tracks(1, 1), playback } })
  const element = mediaElement()
  const { app, player } = await mountPlayer(element as unknown as HTMLAudioElement)
  try {
    await vi.waitFor(() => expect(audioPlayback.value.path).toBe('/audio/1'))
    element.currentTime = 30.5
    player.playItem(tracks(1, 1)[0]!)
    await vi.waitFor(() => expect(element.play).toHaveBeenCalledOnce())
    expect(element.currentTime).toBe(0)
  } finally { app.unmount() }
})
