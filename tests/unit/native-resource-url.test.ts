import { beforeEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({ invoke: vi.fn(), local: true, peer: 'remote', revision: 0, scope: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('../../src/lib/device/local-mode', () => ({ isLocalMode: () => fixture.local }))
vi.mock('../../src/lib/api/native-client', () => ({ nativeViewScope: fixture.scope }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => fixture.peer }))
vi.mock('@/lib/device/login-peers', () => ({ getLoginPeerRevision: () => fixture.revision }))

const resource = { id: 'handle', url: 'plain-resource://file/handle', name: 'fixture', mime: 'text/plain', size: 4, inlineAllowed: true }

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  fixture.local = true
  fixture.peer = 'remote'
  fixture.revision = 0
  vi.stubGlobal('__IS_TAURI__', true)
  fixture.scope.mockResolvedValue('document')
  fixture.invoke.mockImplementation(async (command: string) => command === 'native_resource_open' ? resource : true)
})

it('binds resource open and close to the same document scope', async () => {
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl, dispose: closeNativeResource } = createNativeResourceOwner()
  expect(await nativeResourceUrl('/fixture')).toBe(resource.url)
  expect(await nativeResourceUrl('/fixture')).toBe(resource.url)
  expect(fixture.invoke).toHaveBeenCalledTimes(1)
  expect(fixture.invoke).toHaveBeenCalledWith('native_resource_open', { path: '/fixture', scope: 'document', thumbnail: undefined, target: { kind: 'local' } })
  await closeNativeResource()
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})

it('removes rejected pending opens so an explicit retry can succeed', async () => {
  fixture.invoke.mockRejectedValueOnce(new Error('file unavailable')).mockResolvedValue(resource)
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  await expect(nativeResourceUrl('/fixture')).rejects.toThrow('file unavailable')
  await expect(nativeResourceUrl('/fixture')).resolves.toBe(resource.url)
  expect(fixture.invoke).toHaveBeenCalledTimes(2)
})

it('closes a handle allocated after its consumer has gone away', async () => {
  let finish!: (value: typeof resource) => void
  fixture.invoke.mockImplementation((command: string) => command === 'native_resource_open'
    ? new Promise(resolve => { finish = resolve }) : Promise.resolve(true))
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl, dispose: closeNativeResource } = createNativeResourceOwner()
  const opened = nativeResourceUrl('/fixture')
  const rejected = expect(opened).rejects.toThrow('resource_closed')
  await vi.waitFor(() => expect(finish).toBeDefined())
  await closeNativeResource()
  finish(resource)
  await rejected
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})

it('does not register a local handle after the selected target changes', async () => {
  let finish!: (value: typeof resource) => void
  fixture.invoke.mockImplementation((command: string) => command === 'native_resource_open'
    ? new Promise(resolve => { finish = resolve }) : Promise.resolve(true))
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  const opened = nativeResourceUrl('/fixture')
  const rejected = expect(opened).rejects.toThrow('request_context_changed')
  await vi.waitFor(() => expect(finish).toBeDefined())
  fixture.local = false
  finish(resource)
  await rejected
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})

it('does not allocate after close while the scope handshake is pending', async () => {
  let finish!: (scope: string) => void
  fixture.scope.mockImplementation(() => new Promise(resolve => { finish = resolve }))
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl, dispose: closeNativeResource } = createNativeResourceOwner()
  const opened = nativeResourceUrl('/fixture')
  const rejected = expect(opened).rejects.toThrow('resource_closed')
  await closeNativeResource()
  finish('document')
  await rejected
  expect(fixture.invoke).not.toHaveBeenCalled()
})

it('recognizes only the native resource origins on each platform', async () => {
  const { isNativeResourceUrl, isExternalReference } = await import('../../src/hooks/native-resource-url')
  for (const url of ['plain-resource://file/handle', 'http://plain-resource.file/handle', 'https://plain-resource.file/handle']) {
    expect(isNativeResourceUrl(url)).toBe(true)
    expect(isExternalReference(url)).toBe(false)
  }
  for (const url of ['plain-resource://other/handle', 'http://plain-resource.file.example/handle', 'http://plain-resource.file:8080/handle', 'plain-resource://user@file/handle', 'not-a-url']) {
    expect(isNativeResourceUrl(url)).toBe(false)
  }
  expect(isExternalReference('https://example.test/image.png')).toBe(true)
})


it('keeps thumbnail sizes and resource versions separate and closes every variant', async () => {
  let index = 0
  fixture.invoke.mockImplementation(async (command: string) => command === 'native_resource_open'
    ? { ...resource, id: `handle-${++index}`, url: `plain-resource://file/${index}` } : true)
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl, dispose: closeNativeResource } = createNativeResourceOwner()
  const small = await nativeResourceUrl('/fixture', undefined, '&size=sm&v=first')
  expect(await nativeResourceUrl('/fixture', undefined, '&v=first&size=sm')).toBe(small)
  const large = await nativeResourceUrl('/fixture', undefined, '&size=lg&v=first')
  const updated = await nativeResourceUrl('/fixture', undefined, '&size=sm&v=second')
  expect(new Set([small, large, updated]).size).toBe(3)
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_open').map(([, args]) => args.thumbnail)).toEqual(['sm', 'lg', 'sm'])
  await closeNativeResource()
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_close')).toHaveLength(3)
})

it('checks the actual opened size before exposing an unknown or stale-size resource', async () => {
  const { createNativeResourceOwner, MAX_NATIVE_RESPONSE_BYTES } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  fixture.invoke.mockResolvedValue({ ...resource, size: MAX_NATIVE_RESPONSE_BYTES + 1 })
  expect(await nativeResourceUrl('/fixture')).toBe('')
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: resource.id, scope: 'document' })
  expect(await nativeResourceUrl('/fixture', 1)).toBe('')
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_open')).toHaveLength(2)
})

it('uses thumbnail output metadata rather than skipping a large original file', async () => {
  const { createNativeResourceOwner, MAX_NATIVE_RESPONSE_BYTES } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  expect(await nativeResourceUrl('/fixture', MAX_NATIVE_RESPONSE_BYTES + 1, '&size=sm')).toBe(resource.url)
  expect(fixture.invoke).toHaveBeenCalledExactlyOnceWith('native_resource_open', { path: '/fixture', scope: 'document', thumbnail: 'sm', target: { kind: 'local' } })
})

it('rejects unknown native transformations before invoking or silently serving the original', async () => {
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  for (const query of ['&size=xl', '&action=transcode', '&unknown=true']) {
    await expect(nativeResourceUrl('/fixture', undefined, query)).rejects.toThrow()
  }
  expect(fixture.invoke).not.toHaveBeenCalled()
})


it('reads editor images in bounded raw IPC windows and closes the independent handle', async () => {
  const bytes = Uint8Array.from({ length: 65536 * 2 + 7 }, (_, index) => index % 251)
  fixture.invoke.mockImplementation(async (command: string, args: any) => {
    if (command === 'native_resource_open') return { ...resource, mime: 'image/png', size: bytes.length }
    if (command === 'native_resource_read') return bytes.slice(args.offset, args.offset + args.length).buffer
    return true
  })
  const { nativeResourceImage } = await import('../../src/hooks/native-resource-url')
  const result = await nativeResourceImage('/fixture', new AbortController().signal)
  expect(result.type).toBe('image/png')
  expect(new Uint8Array(await result.arrayBuffer())).toEqual(bytes)
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_read').map(([, args]) => [args.offset, args.length])).toEqual([[0, 65536], [65536, 65536], [131072, 7]])
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})

it('rejects non-images and oversized editor images before reading any bytes', async () => {
  const { nativeResourceImage, MAX_NATIVE_RESPONSE_BYTES } = await import('../../src/hooks/native-resource-url')
  for (const [mime, size] of [['text/html', 8], ['image/png', MAX_NATIVE_RESPONSE_BYTES + 1]] as const) {
    fixture.invoke.mockResolvedValue({ ...resource, mime, size })
    await expect(nativeResourceImage('/fixture', new AbortController().signal)).rejects.toThrow()
  }
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_read')).toHaveLength(0)
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_close')).toHaveLength(2)
})

it('closes a late editor open after cancellation and never reads from it', async () => {
  let finish!: (value: typeof resource) => void
  fixture.invoke.mockImplementation((command: string) => command === 'native_resource_open' ? new Promise(resolve => { finish = resolve }) : Promise.resolve(true))
  const { nativeResourceImage } = await import('../../src/hooks/native-resource-url')
  const controller = new AbortController()
  const reading = nativeResourceImage('/fixture', controller.signal)
  const rejected = expect(reading).rejects.toThrow()
  await vi.waitFor(() => expect(finish).toBeDefined())
  controller.abort()
  finish({ ...resource, mime: 'image/png' })
  await rejected
  expect(fixture.invoke.mock.calls.map(([command]) => command)).toEqual(['native_resource_open', 'native_resource_close'])
})


it('keeps active documents on their existing preview transport before exposing a native URL', async () => {
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  fixture.invoke.mockResolvedValue({ ...resource, mime: 'image/svg+xml', inlineAllowed: false })
  expect(await nativeResourceUrl('/fixture.svg')).toBe('')
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})


it('opens eligible peer images through typed IPC without returning peer URLs or credentials', async () => {
  fixture.local = false
  fixture.invoke.mockResolvedValue({ ...resource, mime: 'image/png' })
  const { createNativeResourceOwner, nativeResourceReference } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  expect(await nativeResourceUrl('/peer/image.png')).toBe(resource.url)
  expect(fixture.invoke).toHaveBeenCalledExactlyOnceWith('native_resource_open', { path: '/peer/image.png', scope: 'document', thumbnail: undefined, target: { kind: 'peer', peerId: 'remote' } })
  expect(nativeResourceReference(resource.url)).toEqual({ path: '/peer/image.png', target: { kind: 'peer', peerId: 'remote' } })
})

it('does not reuse a peer handle after its authorization revision changes', async () => {
  fixture.local = false
  let id = 0
  fixture.invoke.mockImplementation(async (command: string) => command === 'native_resource_open' ? { ...resource, mime: 'image/png', url: `plain-resource://file/${++id}` } : true)
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  const first = await nativeResourceUrl('/peer/image.png')
  fixture.revision++
  expect(await nativeResourceUrl('/peer/image.png')).not.toBe(first)
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_open')).toHaveLength(2)
})

it('keeps peer audio, video and conversion-dependent images on their existing preview transport', async () => {
  fixture.local = false
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  for (const mime of ['video/mp4', 'audio/mpeg', 'image/heic', 'image/tiff', 'text/plain']) {
    fixture.invoke.mockResolvedValue({ ...resource, mime })
    expect(await nativeResourceUrl('/peer/media')).toBe('')
  }
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_close')).toHaveLength(5)
})


it('leaves provider and chat attachment references on their existing transport before opening native handles', async () => {
  fixture.local = false
  const { createNativeResourceOwner } = await import('../../src/hooks/native-resource-url')
  const { url: nativeResourceUrl } = createNativeResourceOwner()
  for (const path of ['content://media/images/1', 'ph://image', 'fsid:attachment', 'https://example.invalid/image']) {
    expect(await nativeResourceUrl(path)).toBe('')
  }
  expect(fixture.invoke).not.toHaveBeenCalled()
})


it.each(['IMAGE/PNG; charset=binary', 'Image/SVG+XML; charset=utf-8'])('normalizes editor image MIME %s without treating it as a document', async (mime) => {
  fixture.invoke.mockImplementation(async (command: string) => {
    if (command === 'native_resource_open') return { ...resource, mime, size: 1, inlineAllowed: false }
    if (command === 'native_resource_read') return new Uint8Array([1]).buffer
    return true
  })
  const { nativeResourceImage } = await import('../../src/hooks/native-resource-url')
  const blob = await nativeResourceImage('/fixture', new AbortController().signal)
  expect(blob.type).toBe(mime.split(';')[0].toLowerCase())
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})

it('shares one exact handle across owners, with idempotent acquisition and last-owner disposal', async () => {
  const { createNativeResourceOwner } = await import('@/hooks/native-resource-url')
  const first = createNativeResourceOwner(), second = createNativeResourceOwner()
  const urls = await Promise.all([first.url('/shared'), first.url('/shared'), second.url('/shared')])
  expect(new Set(urls).size).toBe(1)
  expect(fixture.invoke).toHaveBeenCalledTimes(1)
  await first.dispose(); await first.dispose()
  expect(fixture.invoke).toHaveBeenCalledTimes(1)
  await expect(first.url('/shared')).rejects.toThrow('resource_closed')
  expect(await second.url('/shared')).toBe(resource.url)
  await second.dispose()
  expect(fixture.invoke).toHaveBeenCalledTimes(2)
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})

it('keeps a shared pending open alive for its surviving owner without delivering to a disposed owner', async () => {
  let finish!: (value: typeof resource) => void
  fixture.invoke.mockImplementation((command: string) => command === 'native_resource_open' ? new Promise(resolve => { finish = resolve }) : Promise.resolve(true))
  const { createNativeResourceOwner } = await import('@/hooks/native-resource-url')
  const first = createNativeResourceOwner(), second = createNativeResourceOwner()
  const abandoned = first.url('/shared')
  const rejected = expect(abandoned).rejects.toThrow('resource_closed')
  const retained = second.url('/shared')
  await vi.waitFor(() => expect(finish).toBeDefined())
  await first.dispose()
  finish(resource)
  await rejected
  expect(await retained).toBe(resource.url)
  expect(fixture.invoke).toHaveBeenCalledTimes(1)
  await second.dispose()
  expect(fixture.invoke).toHaveBeenCalledTimes(2)
})

it('does not close other target, revision or thumbnail variants for the same path', async () => {
  let next = 0
  fixture.invoke.mockImplementation(async (command: string) => command === 'native_resource_open'
    ? { ...resource, mime: 'image/png', id: String(++next), url: `plain-resource://file/${next}` } : true)
  const { createNativeResourceOwner, nativeResourceReference } = await import('@/hooks/native-resource-url')
  const first = createNativeResourceOwner(), second = createNativeResourceOwner()
  await first.url('/shared')
  const thumb = await second.url('/shared', undefined, '&size=sm')
  fixture.local = false
  const remote = await second.url('/shared')
  fixture.revision++
  const renewed = await second.url('/shared')
  await first.dispose()
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_close').map(([, args]) => args.id)).toEqual(['1'])
  for (const url of [thumb, remote, renewed]) expect(nativeResourceReference(url).path).toBe('/shared')
  await second.dispose()
  expect(fixture.invoke.mock.calls.filter(([command]) => command === 'native_resource_close')).toHaveLength(4)
})

it('can retain an existing opaque URL without opening a second handle', async () => {
  const { createNativeResourceOwner, nativeResourceReference } = await import('@/hooks/native-resource-url')
  const preview = createNativeResourceOwner(), lightbox = createNativeResourceOwner()
  const url = await preview.url('/shared')
  lightbox.retain(url); lightbox.retain(url)
  await preview.dispose()
  expect(nativeResourceReference(url).path).toBe('/shared')
  expect(fixture.invoke).toHaveBeenCalledTimes(1)
  await lightbox.dispose()
  expect(() => nativeResourceReference(url)).toThrow('no longer available')
  expect(() => lightbox.retain(url)).toThrow()
})
