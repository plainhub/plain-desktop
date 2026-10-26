import { effectScope, ref } from 'vue'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ configure: vi.fn(), close: vi.fn(), fetch: vi.fn() }))
vi.mock('@/lib/mirror-codec-video', () => ({ ScreenMirrorVideoPipeline: class {
  attach() {}
  configure = mock.configure
  close = mock.close
  setOnError() {}
  setOnRequestKeyFrame() {}
  setOnFirstFrameRendered() {}
  requestIdr() {}
} }))
vi.mock('@/lib/mirror-codec-audio', () => ({ ScreenMirrorAudioPipeline: class { attach() {} close() {} prepare() {} } }))
vi.mock('@/lib/mirror-codec-support', () => ({ isVideoDecodeSupported: () => true, isAudioDecodeSupported: () => true, webCodecsMissing: () => '' }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: mock.fetch }))
vi.mock('@/lib/api/query', () => ({ screenMirrorVideoCodecGQL: 'codec', requestScreenMirrorKeyFrameGQL: 'keyframe' }))
import { useScreenMirrorPipeline } from '../../src/views/screen-mirror/screen-mirror-pipeline'

beforeEach(() => vi.clearAllMocks())
afterEach(() => vi.restoreAllMocks())

it.each([false, true])('ignores a pending codec request after cleanup (reject=%s)', async (reject) => {
  let resolve!: (value: unknown) => void
  let fail!: (reason: Error) => void
  mock.fetch.mockImplementationOnce(() => new Promise((yes, no) => { resolve = yes; fail = no }))
  const disconnected = vi.fn()
  const off = vi.fn()
  const scope = effectScope()
  const pipeline = scope.run(() => useScreenMirrorPipeline(ref({} as HTMLCanvasElement), ref(), vi.fn(), disconnected, off))!
  const pending = pipeline.connect()
  pipeline.cleanup()
  if (reject) fail(new Error('closed session'))
  else resolve({ data: { screenMirrorVideoCodec: { annexB: 'AQID' } } })
  await pending
  expect(mock.configure).not.toHaveBeenCalled()
  expect(disconnected).not.toHaveBeenCalled()
  expect(off).not.toHaveBeenCalled()
  mock.fetch.mockResolvedValueOnce({ data: { screenMirrorVideoCodec: { annexB: 'AQID' } } }).mockResolvedValueOnce({ data: {} })
  await pipeline.connect()
  expect(mock.configure).toHaveBeenCalledExactlyOnceWith(new Uint8Array([1, 2, 3]))
  scope.stop()
})
