import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { ISource } from '@/components/lightbox/types'

const fixture = vi.hoisted(() => ({ invoke: vi.fn(), remote: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: fixture.remote }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: vi.fn() }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))

import { openImageEditor, openMediaInWindow } from '@/lib/api/tauri-window'

const source = (src: string, path = 'fid:owned-image'): ISource => ({
  src, path, name: 'image name.png', size: 4, durationMs: 0,
})

beforeEach(() => {
  vi.resetAllMocks()
  fixture.remote.mockReturnValue('other-device')
  vi.stubGlobal('__IS_TAURI__', true)
})
afterEach(() => vi.unstubAllGlobals())

it.each(['plain-resource://file/old-owner', 'http://plain-resource.file/old-owner'])('reopens %s in the preview owner from its original reference', async (url) => {
  const original = { ...source(url), playbackUrl: url, transcoded: true, thumbnail: url }
  await openMediaInWindow(original)
  expect(fixture.invoke).toHaveBeenCalledExactlyOnceWith('media_preview_activate', {
    source: { ...original, src: '', playbackUrl: undefined, transcoded: undefined, thumbnail: undefined },
  })
  expect(original.src).toBe(url)
  expect(fixture.invoke.mock.calls.some(([command]) => command === 'native_resource_close')).toBe(false)
})

it.each(['plain-resource://file/old-owner', 'https://plain-resource.file/old-owner'])('passes only the original reference to a local image-editor window for %s', async (url) => {
  const popup = vi.fn()
  vi.stubGlobal('window', { open: popup })
  await openImageEditor(source(url, 'app://images/image name.png'))
  const [command, args] = fixture.invoke.mock.calls[0]
  expect(command).toBe('open_window')
  const route = new URL(args.path, 'https://fixture.invalid')
  expect(route.pathname).toBe('/image-editor/new')
  expect(route.searchParams.get('reference')).toBe('app://images/image name.png')
  expect(route.searchParams.get('name')).toBe('image name.png')
  expect(route.searchParams.has('src')).toBe(false)
  expect(route.searchParams.has('__cid')).toBe(false)
  expect(popup).not.toHaveBeenCalled()
})

it('refuses a native cross-window handoff without an original reference', async () => {
  for (const path of ['', 'plain-resource://file/opaque']) {
    await expect(openMediaInWindow(source('plain-resource://file/opaque', path))).rejects.toThrow('original file reference')
    await expect(openImageEditor(source('plain-resource://file/opaque', path))).rejects.toThrow('original file reference')
  }
  expect(fixture.invoke).not.toHaveBeenCalled()
})

it('preserves external and remote source URLs instead of treating their paths as local references', async () => {
  const popup = vi.fn()
  vi.stubGlobal('window', { open: popup })
  const remote = source('https://device.invalid/fs?id=encrypted&size=lg', '/remote/image.png')
  await openMediaInWindow(remote)
  expect(fixture.invoke).toHaveBeenCalledExactlyOnceWith('media_preview_activate', { source: remote })
  await openImageEditor(remote)
  const route = new URL(popup.mock.calls[0][0], 'https://fixture.invalid')
  expect(route.searchParams.get('src')).toBe(remote.src)
  expect(route.searchParams.has('reference')).toBe(false)
})

it('does not fetch native media bytes as JSON when HEVC is unavailable', async () => {
  const fetch = vi.fn()
  const canPlayType = vi.fn().mockReturnValue('')
  vi.stubGlobal('fetch', fetch)
  vi.stubGlobal('document', { createElement: () => ({ canPlayType }) })
  const { ensurePlayableVideoUrl } = await import('@/lib/video-codec')
  for (const url of ['plain-resource://file/video', 'http://plain-resource.file/video']) {
    expect(await ensurePlayableVideoUrl(url)).toEqual({ url, transcoded: false })
  }
  expect(fetch).not.toHaveBeenCalled()
  expect(canPlayType).not.toHaveBeenCalled()
})

it('retains HTTP codec probing and HEVC transcoding for remote resources', async () => {
  const fetch = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ codec: 'hvc1' }) })
  vi.stubGlobal('fetch', fetch)
  vi.stubGlobal('document', { createElement: () => ({ canPlayType: () => '' }) })
  const { ensurePlayableVideoUrl } = await import('@/lib/video-codec')
  const result = await ensurePlayableVideoUrl('https://device.invalid/fs?id=video&action=preview')
  expect(fetch).toHaveBeenCalledExactlyOnceWith('https://device.invalid/fs?id=video&action=probe')
  expect(result).toEqual({ url: 'https://device.invalid/fs?id=video&action=transcode', transcoded: true })
})
