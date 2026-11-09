import { beforeEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  native: vi.fn(),
  directory: vi.fn(),
  proxy: vi.fn(),
  write: vi.fn(),
  exists: vi.fn(),
  invoke: vi.fn(),
  reference: vi.fn(),
  scope: vi.fn(),
  local: true,
  peer: 'remote',
}))

vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: fixture.native, nativeViewScope: fixture.scope }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => fixture.peer }))
vi.mock('@/hooks/native-resource-url', () => ({
  isNativeResourceUrl: (url: string) => /^(plain-resource:\/\/file\/|https?:\/\/plain-resource\.file\/)/.test(url),
  nativeResourceReference: fixture.reference,
}))
vi.mock('@/lib/api/api', () => ({ getProxyUrl: fixture.proxy }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => fixture.local }))
vi.mock('@tauri-apps/api/path', () => ({ downloadDir: fixture.directory }))
vi.mock('@tauri-apps/plugin-fs', () => ({ writeFile: fixture.write, exists: fixture.exists }))

import { downloadToDir, downloadAs } from '../../src/lib/download-dir'

beforeEach(() => {
  vi.resetAllMocks()
  fixture.local = true
  fixture.peer = 'remote'
  fixture.scope.mockResolvedValue('document')
  fixture.reference.mockReturnValue({ path: 'fid:owned-image', target: { kind: 'local' } })
  fixture.directory.mockResolvedValue('/downloads')
  fixture.proxy.mockImplementation((path: string) => `http://localhost:3210${path}`)
})

it('uses the local shared Rust copy and returns its actual collision-safe path without JS file IO', async () => {
  fixture.native.mockResolvedValue({ data: { copyFileToDirectory: '/downloads/report (1).txt' } })
  const fetch = vi.spyOn(globalThis, 'fetch')
  try {
    expect(await downloadToDir('https://device.invalid/fs?id=encrypted%2Bvalue&action=download', 'report.txt'))
      .toBe('/downloads/report (1).txt')
    expect(fixture.proxy).toHaveBeenCalledWith('/fs?id=encrypted%2Bvalue&action=download')
    expect(fixture.native).toHaveBeenCalledTimes(1)
    expect(fixture.native.mock.calls[0][0].variables).toEqual({
      source: 'http://localhost:3210/fs?id=encrypted%2Bvalue&action=download',
      directory: '/downloads',
      name: 'report.txt',
    })
    expect(fixture.native.mock.calls[0][1]).toBeGreaterThan(30_000)
    expect(fetch).not.toHaveBeenCalled()
    expect(fixture.write).not.toHaveBeenCalled()
    expect(fixture.exists).not.toHaveBeenCalled()
  } finally {
    fetch.mockRestore()
  }
})

it('captures the selected device resource before waiting for the OS Downloads directory', async () => {
  let selected = 'initial'
  fixture.proxy.mockImplementation((path: string) => `http://localhost:3210/${selected}${path}`)
  let resolve!: (directory: string) => void
  let requested!: () => void
  const ready = new Promise<void>((done) => { requested = done })
  fixture.directory.mockImplementation(() => new Promise<string>((done) => { resolve = done; requested() }))
  fixture.native.mockResolvedValue({ data: { copyFileToDirectory: '/downloads/file.txt' } })
  const download = downloadToDir('https://device.invalid/fs?id=source', 'file.txt')
  await ready
  selected = 'replacement'
  resolve('/downloads')
  await download
  expect(fixture.native.mock.calls[0][0].variables.source).toBe('http://localhost:3210/initial/fs?id=source')
})

it('does not report a saved path when Rust refuses or produces no file', async () => {
  for (const response of [
    { errors: [{ message: 'Transfer failed' }] },
    {},
    { data: { copyFileToDirectory: null } },
    { data: { copyFileToDirectory: '' } },
    { data: { copyFileToDirectory: 42 } },
  ]) {
    fixture.native.mockResolvedValueOnce(response)
    await expect(downloadToDir('https://device.invalid/fs?id=source', 'file.txt')).rejects.toThrow()
  }
  expect(fixture.write).not.toHaveBeenCalled()
  expect(fixture.exists).not.toHaveBeenCalled()
})


it.each(['plain-resource://file/owned-handle', 'http://plain-resource.file/owned-handle'])('exports the target-bound original reference for %s without JS file IO', async (url) => {
  fixture.invoke.mockResolvedValue('/downloads/image.png')
  const fetch = vi.spyOn(globalThis, 'fetch')
  try {
    expect(await downloadToDir(url, 'image.png', 'fid:owned-image')).toBe('/downloads/image.png')
    expect(fixture.invoke).toHaveBeenCalledExactlyOnceWith('native_resource_download', {
      path: 'fid:owned-image', target: { kind: 'local' }, scope: 'document', directory: '/downloads', name: 'image.png',
    })
    expect(fixture.proxy).not.toHaveBeenCalled()
    expect(fetch).not.toHaveBeenCalled()
    expect(fixture.write).not.toHaveBeenCalled()
    expect(fixture.native).not.toHaveBeenCalled()
  } finally { fetch.mockRestore() }
})

it('rejects an expired native handle instead of guessing its source from an opaque URL', async () => {
  fixture.reference.mockImplementation(() => { throw new Error('Native resource is no longer available') })
  await expect(downloadToDir('plain-resource://file/expired', 'file.png', 'fid:guessed')).rejects.toThrow('no longer available')
  await expect(downloadAs('plain-resource://file/expired', 'file.png', 'fid:guessed')).rejects.toThrow('no longer available')
  expect(fixture.invoke).not.toHaveBeenCalled()
  expect(fixture.directory).not.toHaveBeenCalled()
})

it('uses the native save dialog with the captured original target', async () => {
  fixture.reference.mockReturnValue({ path: '/peer/image.png', target: { kind: 'peer', peerId: 'image-device' } })
  fixture.invoke.mockResolvedValue('/saved/image.png')
  expect(await downloadAs('http://plain-resource.file/opaque', 'image.png')).toBe('/saved/image.png')
  expect(fixture.invoke).toHaveBeenCalledExactlyOnceWith('native_resource_save_as', {
    path: '/peer/image.png', target: { kind: 'peer', peerId: 'image-device' }, scope: 'document', name: 'image.png',
  })
  expect(fixture.proxy).not.toHaveBeenCalled()
  expect(fixture.native).not.toHaveBeenCalled()
})

it('downloads large original references through Rust even when their preview is still HTTP', async () => {
  fixture.local = false
  fixture.invoke.mockResolvedValue('/downloads/large.mp4')
  expect(await downloadToDir('https://device.invalid/fs?id=large', 'large.mp4', { path: '/peer/large.mp4', target: { kind: 'peer', peerId: 'remote' } })).toBe('/downloads/large.mp4')
  expect(fixture.invoke).toHaveBeenCalledExactlyOnceWith('native_resource_download', {
    path: '/peer/large.mp4', target: { kind: 'peer', peerId: 'remote' }, scope: 'document', directory: '/downloads', name: 'large.mp4',
  })
  expect(fixture.proxy).not.toHaveBeenCalled()
  expect(fixture.write).not.toHaveBeenCalled()
})


it('never guesses a target for an HTTP resource from a cross-device search path', async () => {
  fixture.native.mockResolvedValue({ data: { copyFileToDirectory: '/downloads/image.png' } })
  expect(await downloadToDir('https://device.invalid/fs?id=other-device', 'image.png', '/same/path/on/different/devices.png')).toBe('/downloads/image.png')
  expect(fixture.invoke).not.toHaveBeenCalled()
  expect(fixture.native.mock.calls[0][0].variables.source).toBe('http://localhost:3210/fs?id=other-device')
})
