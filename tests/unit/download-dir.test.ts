import { beforeEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  native: vi.fn(),
  directory: vi.fn(),
  proxy: vi.fn(),
  write: vi.fn(),
  exists: vi.fn(),
  invoke: vi.fn(),
}))

vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: fixture.native }))
vi.mock('@/lib/api/api', () => ({ getProxyUrl: fixture.proxy }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => true }))
vi.mock('@tauri-apps/api/path', () => ({ downloadDir: fixture.directory }))
vi.mock('@tauri-apps/plugin-fs', () => ({ writeFile: fixture.write, exists: fixture.exists }))

import { downloadToDir, downloadAs } from '../../src/lib/download-dir'

beforeEach(() => {
  vi.resetAllMocks()
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


it.each(['plain-resource://file/owned-handle', 'http://plain-resource.file/owned-handle'])('copies the original native reference without forwarding %s to HTTP', async (url) => {
  fixture.invoke.mockResolvedValue(['/owned/attachment.png'])
  fixture.native.mockResolvedValue({ data: { copyFileToDirectory: '/downloads/image.png' } })
  const fetch = vi.spyOn(globalThis, 'fetch')
  try {
    expect(await downloadToDir(url, 'image.png', 'fid:owned-image')).toBe('/downloads/image.png')
    expect(fixture.invoke).toHaveBeenCalledExactlyOnceWith('native_file_paths', { paths: ['fid:owned-image'] })
    expect(fixture.native.mock.calls[0][0].variables).toEqual({ source: '/owned/attachment.png', directory: '/downloads', name: 'image.png' })
    expect(fixture.proxy).not.toHaveBeenCalled()
    expect(fetch).not.toHaveBeenCalled()
    expect(fixture.write).not.toHaveBeenCalled()
  } finally {
    fetch.mockRestore()
  }
})

it('rejects absent, opaque and unresolved original native references before copying or opening a dialog', async () => {
  const url = 'plain-resource://file/opaque'
  for (const reference of [undefined, '', url]) {
    await expect(downloadToDir(url, 'image.png', reference)).rejects.toThrow('original file reference')
    await expect(downloadAs(url, 'image.png', reference)).rejects.toThrow('original file reference')
  }
  fixture.invoke.mockResolvedValue([null])
  await expect(downloadToDir(url, 'image.png', 'fid:missing')).rejects.toThrow('could not be resolved')
  expect(fixture.proxy).not.toHaveBeenCalled()
  expect(fixture.native).not.toHaveBeenCalled()
  expect(fixture.directory).not.toHaveBeenCalled()
})

it('uses the existing local Save As operation for native references', async () => {
  fixture.invoke.mockResolvedValueOnce(['/owned/image.png']).mockResolvedValueOnce(undefined)
  expect(await downloadAs('http://plain-resource.file/opaque', 'image.png', 'app://image.png')).toBeNull()
  expect(fixture.invoke.mock.calls).toEqual([
    ['native_file_paths', { paths: ['app://image.png'] }],
    ['save_file_as', { path: '/owned/image.png', name: 'image.png' }],
  ])
  expect(fixture.proxy).not.toHaveBeenCalled()
  expect(fixture.native).not.toHaveBeenCalled()
  expect(fixture.directory).not.toHaveBeenCalled()
})
