import { afterEach, beforeEach, expect, it, vi } from 'vitest'
const io = vi.hoisted(() => ({ context: 'owner-a', fetch: vi.fn(), emit: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: io.fetch, getGraphqlContextKey: () => io.context, GqlError: class extends Error {} }))
vi.mock('@/lib/api/query', () => ({ mediaPlayHistoryGQL: 'history' }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: io.emit } }))
import { useMediaPlayHistory } from '@/hooks/media-play-history'

class MediaElement extends EventTarget {
  src = 'https://example.test/fs?id=owned'
  currentSrc = this.src
  currentTime = 0
  ended = false
  play = vi.fn(async () => { this.dispatchEvent(new Event('playing')) })
}
const element = () => new MediaElement() as unknown as HTMLMediaElement & { play: ReturnType<typeof vi.fn> }
const writes = () => io.fetch.mock.calls.filter(([document]) => document !== 'history')
beforeEach(() => {
  vi.useFakeTimers()
  io.context = 'owner-a'
  io.emit.mockReset()
  io.fetch.mockReset().mockImplementation(async (document: string) => ({ data: document === 'history' ? { mediaPlayHistory: [{ positionMs: 18000 }] } : document.includes('mutation recordMediaPlay') ? { recordMediaPlay: true } : { saveMediaPlayProgress: true } }))
})
afterEach(() => vi.useRealTimers())

it.each(['AUDIO', 'VIDEO'] as const)('records actual %s play once, samples position and saves pause without duplicating play count', async type => {
  const media = element()
  const history = useMediaPlayHistory()
  const release = history.bind(media, { mediaId: 'owned', type }, () => true)
  expect(writes()).toHaveLength(0)
  media.currentTime = 0.125
  await media.play()
  await vi.advanceTimersByTimeAsync(0)
  expect(writes()[0]![1]).toEqual({ mediaId: 'owned', type, positionMs: 125 })
  media.currentTime = 2.75
  await vi.advanceTimersByTimeAsync(1000)
  media.dispatchEvent(new Event('pause'))
  await media.play()
  await vi.advanceTimersByTimeAsync(0)
  expect(writes().filter(([document]) => document.includes('mutation recordMediaPlay'))).toHaveLength(1)
  expect(writes().at(-1)![1]).toEqual({ mediaId: 'owned', type, positionMs: 2750 })
  release()
  const count = writes().length
  await vi.advanceTimersByTimeAsync(3000)
  await media.play()
  expect(writes()).toHaveLength(count)
})

it('restores a video before play and records the resumed position from the actual playing event', async () => {
  const media = element()
  const release = await useMediaPlayHistory().restoreAndPlay(media, { mediaId: 'owned', type: 'VIDEO' }, () => true)
  await vi.advanceTimersByTimeAsync(0)
  expect(io.fetch.mock.calls[0]).toEqual(['history', { query: 'type:VIDEO ids:"owned"', offset: 0, limit: 1 }, { fresh: true }])
  expect(media.currentTime).toBe(18)
  expect(media.play).toHaveBeenCalledOnce()
  expect(writes()[0]![1]).toEqual({ mediaId: 'owned', type: 'VIDEO', positionMs: 18000 })
  release?.()
})

it('does not start a retired video after a delayed history response', async () => {
  let finish!: (value: unknown) => void
  io.fetch.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const media = element()
  let active = true
  const start = useMediaPlayHistory().restoreAndPlay(media, { mediaId: 'retired', type: 'VIDEO' }, () => active)
  await vi.advanceTimersByTimeAsync(0)
  active = false
  finish({ data: { mediaPlayHistory: [{ positionMs: 18000 }] } })
  await start
  expect(media.play).not.toHaveBeenCalled()
  expect(media.currentTime).toBe(0)
  expect(writes()).toHaveLength(0)
})

it('keeps anonymous and unindexed playback independent of management history', async () => {
  const media = element()
  await useMediaPlayHistory().restoreAndPlay(media, undefined, () => true)
  expect(media.play).toHaveBeenCalledOnce()
  expect(io.fetch).not.toHaveBeenCalled()
})

it('rejects a history read failure before playback and leaves it visible to the caller', async () => {
  io.fetch.mockResolvedValueOnce({ errors: [{ message: 'Access denied' }] })
  const media = element()
  await expect(useMediaPlayHistory().restoreAndPlay(media, { mediaId: 'owned', type: 'VIDEO' }, () => true)).rejects.toThrow('Access denied')
  expect(media.play).not.toHaveBeenCalled()
  expect(writes()).toHaveLength(0)
})

it('waits for pending progress before reading the same video on reopen', async () => {
  let finish!: (value: unknown) => void
  let saved = 0
  io.fetch.mockImplementation(async (document: string, variables: { positionMs?: number }) => {
    if (document === 'history') return { data: { mediaPlayHistory: [{ positionMs: saved }] } }
    if (document.includes('mutation saveMediaPlayProgress')) saved = variables.positionMs!
    return { data: document.includes('mutation recordMediaPlay') ? { recordMediaPlay: true } : { saveMediaPlayProgress: true } }
  })
  io.fetch.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const history = useMediaPlayHistory()
  const first = element()
  const release = history.bind(first, { mediaId: 'owned', type: 'VIDEO' }, () => true)
  await first.play()
  await vi.advanceTimersByTimeAsync(0)
  first.currentTime = 24
  first.dispatchEvent(new Event('pause'))
  release()
  const reopened = element()
  const start = history.restoreAndPlay(reopened, { mediaId: 'owned', type: 'VIDEO' }, () => true)
  await vi.advanceTimersByTimeAsync(0)
  expect(io.fetch.mock.calls.some(([document]) => document === 'history')).toBe(false)
  expect(reopened.play).not.toHaveBeenCalled()
  finish({ data: { recordMediaPlay: true } })
  const stop = await start
  expect(reopened.currentTime).toBe(24)
  expect(reopened.play).toHaveBeenCalledOnce()
  stop?.()
})

it('does not apply a history result from a previous device', async () => {
  let finish!: (value: unknown) => void
  io.fetch.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const media = element()
  const start = useMediaPlayHistory().restoreAndPlay(media, { mediaId: 'owned', type: 'VIDEO' }, () => true)
  await vi.advanceTimersByTimeAsync(0)
  io.context = 'owner-b'
  finish({ data: { mediaPlayHistory: [{ positionMs: 18000 }] } })
  await start
  expect(media.play).not.toHaveBeenCalled()
  expect(media.currentTime).toBe(0)
  expect(writes()).toHaveLength(0)
})

it('releases the binding when the browser refuses playback', async () => {
  const media = element()
  const error = new Error('Browser playback refused')
  media.play.mockRejectedValueOnce(error)
  await expect(useMediaPlayHistory().restoreAndPlay(media, { mediaId: 'owned', type: 'VIDEO' }, () => true)).rejects.toBe(error)
  media.dispatchEvent(new Event('playing'))
  await vi.advanceTimersByTimeAsync(2000)
  expect(writes()).toHaveLength(0)
  expect(vi.getTimerCount()).toBe(0)
})
