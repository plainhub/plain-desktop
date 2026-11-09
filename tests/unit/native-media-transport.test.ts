import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  local: true, mime: 'video/mp4', inlineAllowed: true,
  invoke: vi.fn(), graphql: vi.fn(), proxy: vi.fn(), events: [] as string[],
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('@tauri-apps/api/path', () => ({ downloadDir: async () => '/downloads' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => fixture.local }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => fixture.local ? '' : 'peer' }))
vi.mock('@/lib/device/login-peers', () => ({ getLoginPeerRevision: () => 0 }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: async () => 'document', nativeGraphql: fixture.graphql }))
vi.mock('@/lib/api/api', () => ({ getProxyUrl: fixture.proxy, getApiHeaders: () => ({ 'c-id': 'browser' }), getUrlToken: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn(), toastWithAction: vi.fn() }))
import { createNativeResourceOwner, nativeResourceReference } from '@/hooks/native-resource-url'
import { resolveFileUrlByPath } from '@/lib/api/file'
import { openMediaInWindow } from '@/lib/api/tauri-window'
import { downloadToDir } from '@/lib/download-dir'

const key = new Uint8Array(32).fill(7)
let owner = createNativeResourceOwner()
beforeEach(() => {
  vi.resetAllMocks()
  fixture.local = true; fixture.mime = 'video/mp4'; fixture.inlineAllowed = true; fixture.events = []
  owner = createNativeResourceOwner()
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', { fileIdMap: new Map() })
  fixture.proxy.mockImplementation((path: string) => {
    fixture.events.push('http')
    return `http://127.0.0.1:8910${fixture.local ? '' : '/proxy/peer'}${path}`
  })
  fixture.invoke.mockImplementation(async (command: string) => {
    if (command === 'native_resource_open') {
      fixture.events.push('open')
      return { id: 'media', url: 'plain-resource://file/media', name: 'fixture', mime: fixture.mime, size: 4, inlineAllowed: fixture.inlineAllowed }
    }
    if (command === 'native_resource_close') fixture.events.push('close')
    if (command === 'native_resource_download') return '/downloads/clip.mp4'
    return true
  })
})
afterEach(async () => { await owner.dispose(); vi.unstubAllGlobals() })

it.each([true, false].flatMap(local => ['video/mp4', 'Audio/MPEG; codecs=mp3', ' VIDEO/WEBM ; codecs=vp9', 'audio/ogg'].map(mime => ({ local, mime }))))(
  'selects HTTP before media loading for local=$local, MIME=$mime, and closes the metadata handle', async ({ local, mime }) => {
    fixture.local = local; fixture.mime = mime
    const url = await resolveFileUrlByPath(owner, key, '/fixture/untyped.bin', 4)
    const parsed = new URL(url)
    expect(parsed.protocol).toBe('http:')
    expect(parsed.pathname).toBe(local ? '/fs' : '/proxy/peer/fs')
    expect(parsed.searchParams.get('action')).toBe('preview')
    expect(parsed.searchParams.get('id')).toBeTruthy()
    expect(url).not.toContain('/fixture/untyped.bin')
    expect(fixture.events).toEqual(['open', 'close', 'http'])
    expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'media', scope: 'document' })
    expect(() => nativeResourceReference('plain-resource://file/media')).toThrow('no longer available')
  },
)

it.each([true, false])('keeps passive image thumbnails native even for an audio/video filename, local=%s', async local => {
  fixture.local = local; fixture.mime = 'IMAGE/PNG; charset=binary'
  const url = await resolveFileUrlByPath(owner, key, '/fixture/clip.mp4', 4, '&size=sm')
  expect(url).toBe('plain-resource://file/media')
  expect(fixture.proxy).not.toHaveBeenCalled()
  expect(fixture.invoke).toHaveBeenCalledExactlyOnceWith('native_resource_open', {
    path: '/fixture/clip.mp4', scope: 'document', thumbnail: 'sm', target: local ? { kind: 'local' } : { kind: 'peer', peerId: 'peer' },
  })
})

it.each(['text/html', 'image/svg+xml', 'application/xml'])('preserves the HTTP preview boundary for active MIME %s', async mime => {
  fixture.mime = mime; fixture.inlineAllowed = false
  expect(await resolveFileUrlByPath(owner, key, '/fixture/document', 4)).toMatch(/^http:/)
  expect(fixture.events).toEqual(['open', 'close', 'http'])
})

it('keeps the existing local non-media native path', async () => {
  fixture.mime = 'text/plain'
  expect(await resolveFileUrlByPath(owner, key, '/fixture/document.txt', 4)).toBe('plain-resource://file/media')
  expect(fixture.proxy).not.toHaveBeenCalled()
})

it.each([true, false])('passes the selected HTTP media URL and original target into a popup, local=%s', async local => {
  fixture.local = local
  const src = await resolveFileUrlByPath(owner, key, '/fixture/clip.mp4', 4)
  const source = { src, path: '/fixture/clip.mp4', name: 'clip.mp4', size: 4, durationMs: 1000 }
  await openMediaInWindow(source)
  expect(fixture.invoke).toHaveBeenLastCalledWith('media_preview_activate', { source: { ...source, __cid: local ? '' : 'peer' } })
  expect(src).toMatch(/^http:/)
})

it.each([true, false])('keeps explicit-target file export native while media preview uses HTTP, local=%s', async local => {
  fixture.local = local
  const src = await resolveFileUrlByPath(owner, key, '/fixture/clip.mp4', 4)
  const target = local ? { kind: 'local' as const } : { kind: 'peer' as const, peerId: 'peer' }
  expect(await downloadToDir(src, 'clip.mp4', { path: '/fixture/clip.mp4', target })).toBe('/downloads/clip.mp4')
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_download', { path: '/fixture/clip.mp4', target, scope: 'document', directory: '/downloads', name: 'clip.mp4' })
  expect(fixture.graphql).not.toHaveBeenCalled()
})

it('does not convert a resource authorization failure into HTTP', async () => {
  fixture.invoke.mockRejectedValueOnce(new Error('unauthorized'))
  await expect(resolveFileUrlByPath(owner, key, '/fixture/clip.mp4', 4)).rejects.toThrow('unauthorized')
  expect(fixture.proxy).not.toHaveBeenCalled()
})
