import { describe, expect, it, vi, beforeEach } from 'vitest'
import { joinPath, uniqueDownloadPath, downloadToDir, downloadAs } from '@/lib/download-dir'

vi.mock('@/lib/api/api', () => ({
  getProxyUrl: (path: string) => `https://proxy.local${path}`,
}))

vi.mock('@tauri-apps/api/path', () => ({
  downloadDir: async () => '/sys/Downloads',
}))

vi.mock('@tauri-apps/plugin-fs', () => ({
  exists: async (p: string) =>
    ((globalThis as any).__dlTaken as Set<string> | undefined)?.has(p) ?? false,
  writeFile: async (p: string, data: unknown) => {
    ;(globalThis as any).__dlWritten.push([p, data])
  },
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: async (opts: { defaultPath: string }) => {
    ;(globalThis as any).__dlSaveOptions = opts
    return ((globalThis as any).__dlSavePath as string | null | undefined) ?? null
  },
}))

describe('joinPath', () => {
  it('joins dir and name with a single slash', () => {
    expect(joinPath('/Users/mac/Downloads', 'a.jpg')).toBe('/Users/mac/Downloads/a.jpg')
    expect(joinPath('/Users/mac/Downloads/', 'a.jpg')).toBe('/Users/mac/Downloads/a.jpg')
    expect(joinPath('/Users/mac/Downloads//', 'a.jpg')).toBe('/Users/mac/Downloads/a.jpg')
  })

  it('keeps windows separators trimmed', () => {
    expect(joinPath('C:\\Users\\mac\\Downloads\\', 'a.jpg')).toBe('C:\\Users\\mac\\Downloads/a.jpg')
  })
})

describe('uniqueDownloadPath', () => {
  it('returns the plain path when it does not exist', async () => {
    const path = await uniqueDownloadPath('/dl', 'a.jpg', async () => false)
    expect(path).toBe('/dl/a.jpg')
  })

  it('appends an increasing counter before the extension on collisions', async () => {
    const taken = new Set(['/dl/a.jpg', '/dl/a (2).jpg'])
    const path = await uniqueDownloadPath('/dl', 'a.jpg', async (p) => taken.has(p))
    expect(path).toBe('/dl/a (3).jpg')
  })

  it('handles names without extension', async () => {
    const taken = new Set(['/dl/README'])
    const path = await uniqueDownloadPath('/dl', 'README', async (p) => taken.has(p))
    expect(path).toBe('/dl/README (2)')
  })
})

describe('downloadToDir', () => {
  const state = globalThis as any

  beforeEach(() => {
    state.__dlTaken = undefined
    state.__dlWritten = []
    vi.unstubAllGlobals()
  })

  function stubFetch(status: number) {
    vi.stubGlobal('fetch', vi.fn(async () =>
      status === 200
        ? { ok: true, body: null, arrayBuffer: async () => new ArrayBuffer(3) }
        : { ok: false, status },
    ))
  }

  it('saves into the system Downloads folder without any stored preference', async () => {
    stubFetch(200)

    const path = await downloadToDir('https://proxy.local/fs?id=x', 'a.jpg')

    expect(path).toBe('/sys/Downloads/a.jpg')
    expect(state.__dlWritten).toEqual([['/sys/Downloads/a.jpg', expect.any(Uint8Array)]])
  })

  it('resolves name collisions inside the system Downloads folder', async () => {
    stubFetch(200)
    state.__dlTaken = new Set(['/sys/Downloads/a.jpg'])

    const path = await downloadToDir('https://proxy.local/fs?id=x', 'a.jpg')

    expect(path).toBe('/sys/Downloads/a (2).jpg')
    expect(state.__dlWritten).toEqual([['/sys/Downloads/a (2).jpg', expect.any(Uint8Array)]])
  })

  it('throws on a non-ok response and writes nothing', async () => {
    stubFetch(404)

    await expect(downloadToDir('https://proxy.local/fs?id=x', 'a.jpg')).rejects.toThrow('HTTP 404')
    expect(state.__dlWritten).toEqual([])
  })
})

describe('downloadAs', () => {
  const state = globalThis as any

  beforeEach(() => {
    state.__dlTaken = undefined
    state.__dlWritten = []
    state.__dlSavePath = undefined
    state.__dlSaveOptions = undefined
    vi.unstubAllGlobals()
  })

  function stubFetch(status: number) {
    vi.stubGlobal('fetch', vi.fn(async () =>
      status === 200
        ? { ok: true, body: null, arrayBuffer: async () => new ArrayBuffer(3) }
        : { ok: false, status },
    ))
  }

  it('suggests Downloads/name in the save dialog and writes to the chosen path', async () => {
    stubFetch(200)
    state.__dlSavePath = '/chosen/elsewhere.jpg'

    const path = await downloadAs('https://proxy.local/fs?id=x', 'a.jpg')

    expect(state.__dlSaveOptions.defaultPath).toBe('/sys/Downloads/a.jpg')
    expect(path).toBe('/chosen/elsewhere.jpg')
    expect(state.__dlWritten).toEqual([['/chosen/elsewhere.jpg', expect.any(Uint8Array)]])
  })

  it('writes nothing when the dialog is cancelled', async () => {
    stubFetch(200)
    state.__dlSavePath = null

    const path = await downloadAs('https://proxy.local/fs?id=x', 'a.jpg')

    expect(path).toBeNull()
    expect(state.__dlWritten).toEqual([])
  })
})
