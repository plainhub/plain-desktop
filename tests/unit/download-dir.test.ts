import { beforeEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  native: vi.fn(),
  directory: vi.fn(),
  proxy: vi.fn(),
  write: vi.fn(),
  exists: vi.fn(),
}))

vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: fixture.native }))
vi.mock('@/lib/api/api', () => ({ getProxyUrl: fixture.proxy }))
vi.mock('@tauri-apps/api/path', () => ({ downloadDir: fixture.directory }))
vi.mock('@tauri-apps/plugin-fs', () => ({ writeFile: fixture.write, exists: fixture.exists }))

import { downloadToDir } from '../../src/lib/download-dir'

beforeEach(() => {
  vi.resetAllMocks()
  fixture.directory.mockResolvedValue('/downloads')
  fixture.proxy.mockImplementation((path: string) => `http://localhost:3210${path}`)
})

it('uses the local shared Rust copy and returns its actual collision-safe path without JS file IO', async () => {
  fixture.native.mockResolvedValue({ data: { copyFileToDirectory: '/downloads/report (1).txt' } })
  const fetch = vi.spyOn(globalThis, 'fetch')
  try {
    expect(await downloadToDir('https://device.invalid/fs?id=encrypted%2Bvalue&dl=1', 'report.txt'))
      .toBe('/downloads/report (1).txt')
    expect(fixture.proxy).toHaveBeenCalledWith('/fs?id=encrypted%2Bvalue&dl=1')
    expect(fixture.native).toHaveBeenCalledTimes(1)
    expect(fixture.native.mock.calls[0][0].variables).toEqual({
      source: 'http://localhost:3210/fs?id=encrypted%2Bvalue&dl=1',
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
